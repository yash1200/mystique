//! Mongo find, aggregate, count, distinct, and shell writes.
//!
//! Every command carries `maxTimeMS` from the profile timeout (see
//! [`opts::max_time`]); writes go through [`write`].

use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::time::{Duration, Instant};

use mongodb::bson::{Bson, Document};
use mongodb::ClientSession;
use mystique_connector_api::{
    json_u64, ConnectorError, ConnectorResult, QueryColumn, QueryControl, QueryMode, QueryResult,
    ValidatedQuery, WriteRisk, NO_TIMEOUT,
};
use serde_json::{json, Value as JsonValue};

use super::encode::{bson_to_json, bson_type_label, resolve_db_name};
use super::opts;
use super::query_failed;
use super::write;
use super::MongoSession;
use crate::shell::parse::lower;
use crate::shell::policy::statement_risk;
use crate::shell::{
    parse, AggregateSpec, CountSpec, Executable, FindAndModify, FindSpec, Listing, Modify,
    MongoCall, MongoOp, ReadOptions, ShellWrite, Target,
};

/// How long a find's total count may run on after the find is done.
const COUNT_GRACE: Duration = Duration::from_secs(2);

/// Dropping `fut` is what aborts a Mongo command. Unlike SQL drivers, there is
/// no `RESET` to run after the server interrupt.
pub(super) async fn drive<T, F>(control: &QueryControl, action: F) -> ConnectorResult<T>
where
    F: IntoFuture<Output = Result<T, mongodb::error::Error>>,
{
    let fut = action.into_future();
    tokio::select! {
        biased;
        _ = control.token.cancelled() => Err(ConnectorError::Query("Query cancelled.".into())),
        result = fut => result.map_err(query_failed),
    }
}

/// The pinned tab's transaction. Every driver call takes it when present.
type Txn<'a> = Option<&'a mut ClientSession>;

pub(super) fn reborrow<'a>(txn: &'a mut Txn<'_>) -> Txn<'a> {
    txn.as_mut().map(|session| &mut **session)
}

/// Run a driver action, inside the pinned transaction when there is one.
/// A macro because each action type has its own inherent `.session()`.
macro_rules! in_txn {
    ($control:expr, $txn:expr, $action:expr) => {{
        let action = $action;
        match reborrow($txn) {
            Some(txn) => drive($control, boxed(action.session(txn))).await,
            None => drive($control, action).await,
        }
    }};
}

pub(super) async fn execute_query(
    session: &MongoSession,
    query: &ValidatedQuery,
) -> ConnectorResult<QueryResult> {
    session.ensure_open()?;
    let start = Instant::now();
    match executable(query)? {
        Executable::Listing(Listing::Databases) => list_databases(session, query, start).await,
        Executable::Listing(Listing::Collections) => {
            let db_name = resolve_db_name(session, query)?;
            list_collections(session, &db_name, query, start).await
        }
        Executable::Call(call) => dispatch_call(session, query, call, start, None, false).await,
    }
}

/// Run inside the pinned tab's transaction.
pub(super) async fn execute_in_transaction(
    session: &MongoSession,
    query: &ValidatedQuery,
    txn: &mut ClientSession,
) -> ConnectorResult<QueryResult> {
    session.ensure_open()?;
    let start = Instant::now();
    match executable(query)? {
        Executable::Listing(listing) => Err(not_in_transaction(listing.command())),
        Executable::Call(call) => dispatch_call(session, query, call, start, Some(txn), true).await,
    }
}

/// A pinned tab with no open transaction: calls still use the tab's session,
/// and listings run as usual.
pub(super) async fn execute_on_session(
    session: &MongoSession,
    query: &ValidatedQuery,
    client_session: &mut ClientSession,
) -> ConnectorResult<QueryResult> {
    session.ensure_open()?;
    let start = Instant::now();
    match executable(query)? {
        Executable::Listing(_) => execute_query(session, query).await,
        Executable::Call(call) => {
            dispatch_call(session, query, call, start, Some(client_session), false).await
        }
    }
}

/// Parse once, refuse a write in Read mode with the policy's own risk, then
/// lower to the call to run.
fn executable(query: &ValidatedQuery) -> ConnectorResult<Executable> {
    let stmt = parse(&query.text)?;
    if query.mode == QueryMode::Read && statement_risk(&stmt) != WriteRisk::Read {
        return Err(ConnectorError::Policy(
            "Write operation blocked. This connection is read-only.".into(),
        ));
    }
    lower(&stmt)
}

/// MongoDB refuses these inside a transaction. Running them outside would
/// look like part of it, so refuse instead.
fn not_in_transaction(command: &str) -> ConnectorError {
    ConnectorError::NotSupported(format!(
        "{command} cannot run inside a transaction. Commit or roll back first."
    ))
}

async fn dispatch_call(
    session: &MongoSession,
    query: &ValidatedQuery,
    call: MongoCall,
    start: Instant,
    txn: Txn<'_>,
    in_transaction: bool,
) -> ConnectorResult<QueryResult> {
    let MongoCall { target, op } = call;
    let db_name = match target.database.as_deref() {
        Some(db) => db.to_string(),
        None => resolve_db_name(session, query)?,
    };
    let coll_name = collection_name(&target, query)?;
    let at = At {
        session,
        query,
        db_name: &db_name,
        coll_name: &coll_name,
        in_transaction,
    };
    match op {
        MongoOp::Find(spec) => execute_find(&at, spec, start, txn).await,
        MongoOp::Aggregate(spec) => execute_aggregate(&at, spec, start, txn).await,
        MongoOp::Count(spec) => execute_count(&at, spec, start, txn).await,
        MongoOp::EstimatedCount(read) => execute_estimated_count(&at, read, start).await,
        MongoOp::Distinct {
            field,
            filter,
            read,
        } => execute_distinct(&at, field, filter, read, start, txn).await,
        MongoOp::Write(write) => execute_shell_write(&at, write, start, txn).await,
    }
}

/// Where a parsed call runs.
struct At<'a> {
    session: &'a MongoSession,
    query: &'a ValidatedQuery,
    db_name: &'a str,
    coll_name: &'a str,
    /// The pinned session has an open transaction.
    in_transaction: bool,
}

impl At<'_> {
    fn collection(&self) -> mongodb::Collection<Document> {
        self.session
            .client
            .database(self.db_name)
            .collection::<Document>(self.coll_name)
    }

    /// `db.coll`, for result lines.
    fn name(&self) -> String {
        format!("{}.{}", self.db_name, self.coll_name)
    }
}

/// The future in `slot`, or never when there is none.
async fn pending_or<F: Future + Unpin>(slot: &mut Option<F>) -> F::Output {
    match slot {
        Some(fut) => fut.await,
        None => std::future::pending().await,
    }
}

/// Whether a find should count its matches alongside: not with an explicit
/// limit (or `findOne`), and not on a pinned session, which runs one command
/// at a time.
pub(super) fn counts_alongside(spec: &FindSpec, pinned: bool) -> bool {
    spec.limit.is_none() && !pinned
}

async fn execute_find(
    at: &At<'_>,
    parsed: FindSpec,
    start: Instant,
    txn: Txn<'_>,
) -> ConnectorResult<QueryResult> {
    let query = at.query;
    let coll = at.collection();
    let options = opts::find_options(&parsed, query.max_rows, query.timeout_ms)?;
    // The total runs with the find, not after it. Only a truncated page needs
    // it; otherwise the rows are the total and the count is dropped.
    let mut total = if counts_alongside(&parsed, txn.is_some()) {
        let count = coll
            .count_documents(parsed.filter.clone())
            .with_options(opts::find_count_options(&parsed, query.timeout_ms)?);
        Some(boxed(count))
    } else {
        None
    };
    let mut counted: Option<u64> = None;
    let find = coll.find(parsed.filter).with_options(options);
    let rows = async {
        let cursor = match txn {
            Some(txn) => {
                let cursor = drive(&query.control, boxed(find.session(&mut *txn))).await?;
                AnyCursor::Pinned(cursor, txn)
            }
            None => AnyCursor::Plain(drive(&query.control, find).await?),
        };
        drain_cursor(cursor, query).await
    };
    tokio::pin!(rows);
    let (docs, truncated) = loop {
        tokio::select! {
            result = &mut rows => break result?,
            n = pending_or(&mut total), if total.is_some() => {
                total = None;
                counted = n.ok();
            }
        }
    };
    // A slow count does not hold the page back for long; dropping it aborts it.
    if truncated && counted.is_none() {
        if let Some(count) = total.take() {
            counted = tokio::select! {
                biased;
                _ = query.control.token.cancelled() => None,
                n = tokio::time::timeout(COUNT_GRACE, count) => n.ok().and_then(Result::ok),
            };
        }
    }
    drop(total);
    let mut message = format!("find on {}", at.name());
    if truncated {
        if let Some(n) = counted {
            message.push_str(&format!(" ({n} matching)"));
        }
    }
    docs_result(docs, truncated, query.mode, start, message)
}

fn collection_name(target: &Target, query: &ValidatedQuery) -> ConnectorResult<String> {
    target
        .collection
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| {
            query
                .collection
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(
                "Mongo queries need a collection (db.collection.method(...) or the Collection field)."
                    .into(),
            )
        })
}

async fn execute_aggregate(
    at: &At<'_>,
    spec: AggregateSpec,
    start: Instant,
    txn: Txn<'_>,
) -> ConnectorResult<QueryResult> {
    let query = at.query;
    let coll = at.collection();
    let options = opts::aggregate_options(&spec, query.max_rows, query.timeout_ms)?;
    let action = coll.aggregate(spec.pipeline).with_options(options);
    let cursor = match txn {
        Some(txn) => {
            let cursor = drive(&query.control, boxed(action.session(&mut *txn))).await?;
            AnyCursor::Pinned(cursor, txn)
        }
        None => AnyCursor::Plain(drive(&query.control, action).await?),
    };
    let (docs, truncated) = drain_cursor(cursor, query).await?;
    docs_result(
        docs,
        truncated,
        query.mode,
        start,
        format!("aggregate on {}", at.name()),
    )
}

fn count_result(
    query: &ValidatedQuery,
    count: u64,
    start: Instant,
    message: String,
) -> QueryResult {
    QueryResult {
        engine: "mongodb".into(),
        mode: query.mode,
        columns: vec![QueryColumn {
            name: "count".into(),
            data_type: Some("number".into()),
        }],
        rows: vec![vec![json_u64(count)]],
        row_count: 1,
        truncated: false,
        duration_ms: start.elapsed().as_millis() as u64,
        message: Some(message),
        payload: None,
    }
}

async fn execute_count(
    at: &At<'_>,
    spec: CountSpec,
    start: Instant,
    mut txn: Txn<'_>,
) -> ConnectorResult<QueryResult> {
    let query = at.query;
    let coll = at.collection();
    let options = opts::count_options(&spec, query.timeout_ms)?;
    let count = in_txn!(
        &query.control,
        &mut txn,
        coll.count_documents(spec.filter).with_options(options)
    )?;
    Ok(count_result(
        query,
        count,
        start,
        format!("countDocuments on {}", at.name()),
    ))
}

/// From collection metadata, not a scan. The driver runs it without a
/// session, and MongoDB refuses it in a transaction.
async fn execute_estimated_count(
    at: &At<'_>,
    read: ReadOptions,
    start: Instant,
) -> ConnectorResult<QueryResult> {
    let query = at.query;
    if at.in_transaction {
        return Err(not_in_transaction("estimatedDocumentCount"));
    }
    let coll = at.collection();
    let options = opts::estimated_count_options(&read, query.timeout_ms);
    let count = drive(
        &query.control,
        coll.estimated_document_count().with_options(options),
    )
    .await?;
    Ok(count_result(
        query,
        count,
        start,
        format!("estimatedDocumentCount on {}", at.name()),
    ))
}

async fn execute_distinct(
    at: &At<'_>,
    field: String,
    filter: Document,
    read: ReadOptions,
    start: Instant,
    mut txn: Txn<'_>,
) -> ConnectorResult<QueryResult> {
    let query = at.query;
    let coll = at.collection();
    let options = opts::distinct_options(&read, query.timeout_ms)?;
    let mut values = in_txn!(
        &query.control,
        &mut txn,
        coll.distinct(&field, filter).with_options(options)
    )?;
    let truncated = values.len() as u32 > query.max_rows;
    if truncated {
        values.truncate(query.max_rows as usize);
    }
    let rows = values
        .into_iter()
        .map(|value| vec![bson_to_json(value)])
        .collect::<Vec<_>>();
    let row_count = rows.len() as u64;
    Ok(QueryResult {
        engine: "mongodb".into(),
        mode: query.mode,
        columns: vec![QueryColumn {
            name: "value".into(),
            data_type: Some("bson".into()),
        }],
        rows,
        row_count,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
        message: Some(format!("distinct {field} on {}", at.name())),
        payload: None,
    })
}

async fn execute_shell_write(
    at: &At<'_>,
    write: ShellWrite,
    start: Instant,
    txn: Txn<'_>,
) -> ConnectorResult<QueryResult> {
    let query = at.query;
    match write {
        ShellWrite::Models {
            method,
            models,
            options,
        } => {
            let db = at.session.client.database(at.db_name);
            let message = write::run_models(
                &db,
                &at.name(),
                at.coll_name,
                method,
                models,
                &options,
                query.timeout_ms,
                &query.control,
                txn,
                at.in_transaction,
            )
            .await?;
            Ok(write_message(query.mode, start, message))
        }
        ShellWrite::FindAndModify(spec) => find_and_modify(at, spec, start, txn).await,
    }
}

/// findOneAndUpdate / findOneAndReplace / findOneAndDelete through the driver,
/// which takes `maxTimeMS` and every option for `findAndModify`.
async fn find_and_modify(
    at: &At<'_>,
    spec: FindAndModify,
    start: Instant,
    mut txn: Txn<'_>,
) -> ConnectorResult<QueryResult> {
    let query = at.query;
    let coll = at.collection();
    let method = spec.method();
    let filter = spec.filter.clone();
    let found = match &spec.modify {
        Modify::Update(update) => {
            let options = opts::find_one_and_update_options(&spec, query.timeout_ms)?;
            in_txn!(
                &query.control,
                &mut txn,
                coll.find_one_and_update(filter, update.clone())
                    .with_options(options)
            )?
        }
        Modify::Replace(replacement) => {
            let options = opts::find_one_and_replace_options(&spec, query.timeout_ms)?;
            in_txn!(
                &query.control,
                &mut txn,
                coll.find_one_and_replace(filter, replacement.clone())
                    .with_options(options)
            )?
        }
        Modify::Delete => {
            let options = opts::find_one_and_delete_options(&spec, query.timeout_ms)?;
            in_txn!(
                &query.control,
                &mut txn,
                coll.find_one_and_delete(filter).with_options(options)
            )?
        }
    };
    match found {
        Some(doc) => docs_result(
            vec![doc],
            false,
            query.mode,
            start,
            format!("{method} on {}", at.name()),
        ),
        // With `upsert` and `returnDocument: "before"` an insert returns nothing.
        None if spec.upsert == Some(true) => Ok(write_message(
            query.mode,
            start,
            format!("{method} on {} returned no document", at.name()),
        )),
        None => Ok(write_message(
            query.mode,
            start,
            format!("{method} on {} matched=0", at.name()),
        )),
    }
}

pub(super) type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Box a session-bound driver call. Its future otherwise trips rustc's
/// higher-ranked `Send` check inside `async_trait`.
pub(super) fn boxed<'a, F>(action: F) -> Boxed<'a, F::Output>
where
    F: IntoFuture + 'a,
    F::IntoFuture: Send + 'a,
{
    Box::pin(action.into_future())
}

/// A plain cursor, or one bound to the pinned transaction.
enum AnyCursor<'a> {
    Plain(mongodb::Cursor<Document>),
    Pinned(mongodb::SessionCursor<Document>, &'a mut ClientSession),
}

impl AnyCursor<'_> {
    fn advance(&mut self) -> Boxed<'_, mongodb::error::Result<bool>> {
        match self {
            Self::Plain(cursor) => Box::pin(cursor.advance()),
            Self::Pinned(cursor, txn) => Box::pin(cursor.advance(txn)),
        }
    }

    fn deserialize_current(&self) -> mongodb::error::Result<Document> {
        match self {
            Self::Plain(cursor) => cursor.deserialize_current(),
            Self::Pinned(cursor, _) => cursor.deserialize_current(),
        }
    }
}

async fn drain_cursor(
    mut cursor: AnyCursor<'_>,
    query: &ValidatedQuery,
) -> ConnectorResult<(Vec<Document>, bool)> {
    let mut docs = Vec::new();
    let mut saw_doc = false;
    let mut truncated = false;
    loop {
        let advanced = tokio::select! {
            biased;
            _ = query.control.token.cancelled() => {
                drop(cursor);
                return Err(ConnectorError::Query("Query cancelled.".into()));
            }
            _ = tokio::time::sleep(Duration::from_millis(mystique_connector_api::INACTIVITY_TIMEOUT_MS)), if saw_doc && query.timeout_ms != NO_TIMEOUT => {
                query.control.cancel();
                drop(cursor);
                return Err(ConnectorError::Timeout(format!(
                    "No rows arrived for {} ms.",
                    mystique_connector_api::INACTIVITY_TIMEOUT_MS
                )));
            }
            advanced = cursor.advance() => advanced.map_err(query_failed)?,
        };
        if !advanced {
            break;
        }
        saw_doc = true;
        docs.push(cursor.deserialize_current().map_err(query_failed)?);
        if docs.len() >= query.max_rows as usize {
            truncated = true;
            break;
        }
    }
    Ok((docs, truncated))
}

fn docs_result(
    docs: Vec<Document>,
    truncated: bool,
    mode: QueryMode,
    start: Instant,
    message: String,
) -> ConnectorResult<QueryResult> {
    let mut key_set: Vec<String> = Vec::new();
    for doc in &docs {
        for key in doc.keys() {
            if !key_set.iter().any(|existing| existing == key) {
                key_set.push(key.clone());
            }
        }
    }
    if let Some(index) = key_set.iter().position(|key| key == "_id") {
        let id = key_set.remove(index);
        key_set.insert(0, id);
    }
    let columns: Vec<QueryColumn> = key_set
        .iter()
        .map(|key| {
            let sample = docs.iter().find_map(|doc| doc.get(key)).cloned();
            QueryColumn {
                name: key.clone(),
                data_type: Some(bson_type_label(sample.as_ref())),
            }
        })
        .collect();
    let mut rows = Vec::with_capacity(docs.len());
    for doc in docs {
        let mut cells = Vec::with_capacity(key_set.len());
        for key in &key_set {
            let value = doc.get(key).cloned().unwrap_or(Bson::Null);
            cells.push(bson_to_json(value));
        }
        rows.push(cells);
    }
    let row_count = rows.len() as u64;
    Ok(QueryResult {
        engine: "mongodb".into(),
        mode,
        columns,
        rows,
        row_count,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
        message: Some(message),
        payload: None,
    })
}

fn write_message(mode: QueryMode, start: Instant, message: String) -> QueryResult {
    QueryResult {
        engine: "mongodb".into(),
        mode,
        columns: Vec::new(),
        rows: Vec::new(),
        row_count: 0,
        truncated: false,
        duration_ms: start.elapsed().as_millis() as u64,
        message: Some(message),
        payload: None,
    }
}

pub(super) async fn list_databases(
    session: &MongoSession,
    query: &ValidatedQuery,
    start: Instant,
) -> ConnectorResult<QueryResult> {
    let mode = query.mode;
    let names = drive(&query.control, session.client.list_database_names()).await?;

    let columns = vec![QueryColumn {
        name: "name".into(),
        data_type: Some("string".into()),
    }];
    let rows: Vec<Vec<JsonValue>> = names
        .into_iter()
        .map(|n| vec![JsonValue::String(n)])
        .collect();
    let count = rows.len() as u64;

    Ok(QueryResult {
        engine: "mongodb".into(),
        mode,
        columns,
        row_count: count,
        rows,
        truncated: false,
        duration_ms: start.elapsed().as_millis() as u64,
        message: Some("listDatabases".into()),
        payload: None,
    })
}

pub(super) async fn list_collections(
    session: &MongoSession,
    db_name: &str,
    query: &ValidatedQuery,
    start: Instant,
) -> ConnectorResult<QueryResult> {
    let mode = query.mode;
    let max_rows = query.max_rows;
    let mut names = drive(
        &query.control,
        session.client.database(db_name).list_collection_names(),
    )
    .await?;
    names.sort();
    let truncated = names.len() as u32 > max_rows;
    if truncated {
        names.truncate(max_rows as usize);
    }

    // Estimated counts (best-effort; empty on error)
    let mut rows = Vec::with_capacity(names.len());
    for name in &names {
        if query.control.is_cancelled() {
            return Err(ConnectorError::Query("Query cancelled.".into()));
        }
        let count = drive(
            &query.control,
            session
                .client
                .database(db_name)
                .collection::<Document>(name)
                .estimated_document_count(),
        )
        .await
        .ok();
        rows.push(vec![
            JsonValue::String(name.clone()),
            count.map(|c| json!(c)).unwrap_or(JsonValue::Null),
        ]);
    }

    Ok(QueryResult {
        engine: "mongodb".into(),
        mode,
        columns: vec![
            QueryColumn {
                name: "name".into(),
                data_type: Some("string".into()),
            },
            QueryColumn {
                name: "estimatedCount".into(),
                data_type: Some("number".into()),
            },
        ],
        row_count: rows.len() as u64,
        rows,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
        message: Some(format!("listCollections on {db_name}")),
        payload: None,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use mystique_connector_api::{clamp_timeout_ms, QueryControl, MAX_TIMEOUT_MS, NO_TIMEOUT};

    use super::not_in_transaction;
    use crate::session::opts::max_time;
    use crate::shell::{parse_executable, Executable, Listing};

    fn listing(text: &str) -> Option<Listing> {
        match parse_executable(text) {
            Ok(Executable::Listing(listing)) => Some(listing),
            _ => None,
        }
    }

    fn find(text: &str) -> crate::shell::FindSpec {
        match parse_executable(text) {
            Ok(Executable::Call(crate::shell::MongoCall {
                op: crate::shell::MongoOp::Find(spec),
                ..
            })) => spec,
            other => panic!("{text}: {other:?}"),
        }
    }

    #[test]
    fn a_find_counts_alongside_unless_the_user_set_a_limit() {
        use super::counts_alongside;
        assert!(counts_alongside(&find("db.t.find({a: 1})"), false));
        assert!(counts_alongside(
            &find("db.t.find({}).sort({a: 1}).skip(5)"),
            false
        ));
        assert!(counts_alongside(&find("db.t.find({}).limit(0)"), false));
        assert!(!counts_alongside(&find("db.t.find({}).limit(10)"), false));
        assert!(!counts_alongside(
            &find("db.t.find({}, {}, {limit: 10})"),
            false
        ));
        assert!(!counts_alongside(&find("db.t.findOne({a: 1})"), false));
        // A pinned session runs one command at a time.
        assert!(!counts_alongside(&find("db.t.find({a: 1})"), true));
    }

    #[test]
    fn listing_commands_are_known_so_a_transaction_can_refuse_them() {
        assert_eq!(listing("show dbs"), Some(Listing::Databases));
        assert_eq!(listing("show collections"), Some(Listing::Collections));
        assert_eq!(listing("db.users.find({})"), None);
        let err = not_in_transaction(Listing::Collections.command()).to_string();
        assert!(err.contains("listCollections"), "{err}");
        assert!(err.contains("transaction"), "{err}");
    }

    struct Cursor {
        dropped: Arc<AtomicBool>,
    }

    impl Drop for Cursor {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn cancelled_find_drops_the_cursor() {
        // Command: `find` with `maxTimeMS` equal to the clamped timeout.
        // The cursor is dropped when the cancellation token fires, which aborts
        // the command. No live MongoDB is required for this check.
        assert_eq!(
            max_time(clamp_timeout_ms(Some(120_000)), None),
            Some(Duration::from_millis(120_000))
        );
        assert_eq!(
            max_time(clamp_timeout_ms(Some(u64::MAX)), None),
            Some(Duration::from_millis(MAX_TIMEOUT_MS))
        );
        // No timeout sends no maxTimeMS at all.
        assert_eq!(max_time(clamp_timeout_ms(Some(NO_TIMEOUT)), None), None);

        let control = QueryControl::new();
        let dropped = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&dropped);
        let watched = Arc::clone(&control);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            watched.cancel();
        });
        let cursor = Cursor {
            dropped: Arc::clone(&flag),
        };
        let result = super::drive(&control, async move {
            let _cursor = cursor;
            std::future::pending::<()>().await;
            Ok::<(), mongodb::error::Error>(())
        })
        .await;
        assert!(result.is_err(), "cancel should stop find");
        assert!(
            dropped.load(Ordering::SeqCst),
            "cursor must drop when find is cancelled"
        );
    }
}
