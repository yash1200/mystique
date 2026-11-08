//! Mongo find, aggregate, count, distinct, and shell writes.

use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::time::{Duration, Instant};

use mongodb::bson::{Bson, Document};
use mongodb::ClientSession;
use mystique_connector_api::{
    json_u64, ConnectorError, ConnectorResult, QueryColumn, QueryControl, QueryMode, QueryResult,
    ValidatedQuery,
};
use serde_json::{json, Value as JsonValue};

use super::encode::{bson_to_json, bson_type_label, normalize_meta_command, resolve_db_name};
use super::query_failed;
use super::MongoSession;
use crate::shell_query::{
    aggregate_effect, parse_mongo_call, AggregateEffect, BulkModel, MongoCall, ParsedFind,
    ShellWrite,
};

/// `find` is sent with `maxTimeMS` equal to this duration (the clamped timeout).
pub(super) fn find_max_time(timeout_ms: u64) -> Duration {
    Duration::from_millis(timeout_ms.max(1))
}

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

fn reborrow<'a>(txn: &'a mut Txn<'_>) -> Txn<'a> {
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
    match listing(&query.text) {
        Some(Listing::Databases) => list_databases(session, query, start).await,
        Some(Listing::Collections) => {
            let db_name = resolve_db_name(session, query)?;
            list_collections(session, &db_name, query, start).await
        }
        None => {
            let call = parse_mongo_call(&query.text)?;
            dispatch_call(session, query, call, start, None).await
        }
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
    if let Some(listing) = listing(&query.text) {
        return Err(not_in_transaction(listing.command()));
    }
    let call = parse_mongo_call(&query.text)?;
    dispatch_call(session, query, call, start, Some(txn)).await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Listing {
    Databases,
    Collections,
}

impl Listing {
    fn command(self) -> &'static str {
        match self {
            Self::Databases => "listDatabases",
            Self::Collections => "listCollections",
        }
    }
}

fn listing(text: &str) -> Option<Listing> {
    match normalize_meta_command(text).as_str() {
        "listdatabases" | "showdatabases" | "showdbs" => Some(Listing::Databases),
        "listcollections" | "showcollections" => Some(Listing::Collections),
        _ => None,
    }
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
) -> ConnectorResult<QueryResult> {
    match call {
        MongoCall::Find(parsed) => execute_find(session, query, parsed, start, txn).await,
        MongoCall::Aggregate(aggregate) => {
            execute_aggregate(session, query, aggregate, start, txn).await
        }
        MongoCall::Count { collection, filter } => {
            execute_count(session, query, collection, filter, start, txn).await
        }
        MongoCall::Distinct {
            collection,
            field,
            filter,
        } => execute_distinct(session, query, collection, field, filter, start, txn).await,
        MongoCall::Write(write) => {
            if query.mode == QueryMode::Read {
                return Err(ConnectorError::Policy(
                    "Write operation blocked. This connection is read-only.".into(),
                ));
            }
            execute_shell_write(session, query, write, start, txn).await
        }
    }
}

async fn execute_find(
    session: &MongoSession,
    query: &ValidatedQuery,
    parsed: ParsedFind,
    start: Instant,
    txn: Txn<'_>,
) -> ConnectorResult<QueryResult> {
    let db_name = resolve_db_name(session, query)?;
    let coll_name = parsed
        .collection
        .clone()
        .or_else(|| {
            query
                .collection
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(
                "Mongo queries need a collection (use the Collection field or db.collection.find(...))."
                    .into(),
            )
        })?;

    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&coll_name);
    let limit = parsed
        .limit
        .unwrap_or(query.max_rows as i64)
        .clamp(1, query.max_rows as i64);
    let mut find = coll.find(parsed.filter);
    if let Some(proj) = parsed.projection {
        find = find.projection(proj);
    }
    if let Some(sort) = parsed.sort {
        find = find.sort(sort);
    }
    if let Some(skip) = parsed.skip {
        find = find.skip(skip);
    }
    let max_time = find_max_time(query.timeout_ms);
    let find = find.limit(limit).max_time(max_time);
    let cursor = match txn {
        Some(txn) => {
            let cursor = drive(&query.control, boxed(find.session(&mut *txn))).await?;
            AnyCursor::Pinned(cursor, txn)
        }
        None => AnyCursor::Plain(drive(&query.control, find).await?),
    };
    let (docs, _) = drain_cursor(cursor, query).await?;

    // Flatten documents into a table: union of keys
    let mut key_set: Vec<String> = Vec::new();
    for d in &docs {
        for k in d.keys() {
            if !key_set.iter().any(|x| x == k) {
                key_set.push(k.clone());
            }
        }
    }
    // Prefer _id first
    if let Some(i) = key_set.iter().position(|k| k == "_id") {
        let id = key_set.remove(i);
        key_set.insert(0, id);
    }

    let columns: Vec<QueryColumn> = key_set
        .iter()
        .map(|k| {
            let sample = docs.iter().find_map(|d| d.get(k)).cloned();
            QueryColumn {
                name: k.clone(),
                data_type: Some(bson_type_label(sample.as_ref())),
            }
        })
        .collect();

    let mut rows = Vec::new();
    for d in docs {
        let mut cells = Vec::with_capacity(key_set.len());
        for k in &key_set {
            let v = d.get(k).cloned().unwrap_or(Bson::Null);
            cells.push(bson_to_json(v));
        }
        rows.push(cells);
    }

    let truncated = rows.len() as u32 >= query.max_rows;
    Ok(QueryResult {
        engine: "mongodb".into(),
        mode: query.mode,
        columns,
        row_count: rows.len() as u64,
        rows,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
        message: Some(format!("find on {db_name}.{coll_name}")),
    })
}

fn collection_name(parsed: Option<&str>, query: &ValidatedQuery) -> ConnectorResult<String> {
    parsed
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

fn reject_read_only_mutation(effect: AggregateEffect, mode: QueryMode) -> ConnectorResult<()> {
    if mode == QueryMode::Read && effect != AggregateEffect::Read {
        return Err(ConnectorError::Policy(
            "Write operation blocked. This connection is read-only.".into(),
        ));
    }
    Ok(())
}

async fn execute_aggregate(
    session: &MongoSession,
    query: &ValidatedQuery,
    aggregate: crate::shell_query::ParsedAggregate,
    start: Instant,
    txn: Txn<'_>,
) -> ConnectorResult<QueryResult> {
    reject_read_only_mutation(aggregate_effect(&aggregate.pipeline), query.mode)?;
    let db_name = resolve_db_name(session, query)?;
    let coll_name = collection_name(aggregate.collection.as_deref(), query)?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&coll_name);
    let batch = query.max_rows.clamp(1, 1_000);
    let action = coll
        .aggregate(aggregate.pipeline)
        .batch_size(batch)
        .max_time(find_max_time(query.timeout_ms));
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
        format!("aggregate on {db_name}.{coll_name}"),
    )
}

async fn execute_count(
    session: &MongoSession,
    query: &ValidatedQuery,
    collection: Option<String>,
    filter: Document,
    start: Instant,
    mut txn: Txn<'_>,
) -> ConnectorResult<QueryResult> {
    let db_name = resolve_db_name(session, query)?;
    let coll_name = collection_name(collection.as_deref(), query)?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&coll_name);
    let count = in_txn!(
        &query.control,
        &mut txn,
        coll.count_documents(filter)
            .max_time(find_max_time(query.timeout_ms))
    )?;
    Ok(QueryResult {
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
        message: Some(format!("countDocuments on {db_name}.{coll_name}")),
    })
}

async fn execute_distinct(
    session: &MongoSession,
    query: &ValidatedQuery,
    collection: Option<String>,
    field: String,
    filter: Document,
    start: Instant,
    mut txn: Txn<'_>,
) -> ConnectorResult<QueryResult> {
    let db_name = resolve_db_name(session, query)?;
    let coll_name = collection_name(collection.as_deref(), query)?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&coll_name);
    let mut values = in_txn!(
        &query.control,
        &mut txn,
        coll.distinct(&field, filter)
            .max_time(find_max_time(query.timeout_ms))
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
        message: Some(format!("distinct {field} on {db_name}.{coll_name}")),
    })
}

async fn execute_shell_write(
    session: &MongoSession,
    query: &ValidatedQuery,
    write: ShellWrite,
    start: Instant,
    mut txn: Txn<'_>,
) -> ConnectorResult<QueryResult> {
    let db_name = resolve_db_name(session, query)?;
    let collection = match &write {
        ShellWrite::InsertOne { collection, .. }
        | ShellWrite::InsertMany { collection, .. }
        | ShellWrite::UpdateOne { collection, .. }
        | ShellWrite::UpdateMany { collection, .. }
        | ShellWrite::ReplaceOne { collection, .. }
        | ShellWrite::DeleteOne { collection, .. }
        | ShellWrite::DeleteMany { collection, .. }
        | ShellWrite::FindOneAndUpdate { collection, .. }
        | ShellWrite::Bulk { collection, .. } => collection.as_deref(),
    };
    let coll_name = collection_name(collection, query)?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&coll_name);
    let message = match write {
        ShellWrite::InsertOne { document, .. } => {
            let result = in_txn!(&query.control, &mut txn, coll.insert_one(document))?;
            format!(
                "insertOne on {db_name}.{coll_name} id={:?}",
                result.inserted_id
            )
        }
        ShellWrite::InsertMany { documents, .. } => {
            let result = in_txn!(&query.control, &mut txn, coll.insert_many(documents))?;
            format!(
                "insertMany on {db_name}.{coll_name} inserted={}",
                result.inserted_ids.len()
            )
        }
        ShellWrite::UpdateOne { filter, update, .. } => {
            let result = in_txn!(&query.control, &mut txn, coll.update_one(filter, update))?;
            format!(
                "updateOne on {db_name}.{coll_name} matched={} modified={}",
                result.matched_count, result.modified_count
            )
        }
        ShellWrite::UpdateMany { filter, update, .. } => {
            let result = in_txn!(&query.control, &mut txn, coll.update_many(filter, update))?;
            format!(
                "updateMany on {db_name}.{coll_name} matched={} modified={}",
                result.matched_count, result.modified_count
            )
        }
        ShellWrite::ReplaceOne {
            filter,
            replacement,
            ..
        } => {
            let result = in_txn!(
                &query.control,
                &mut txn,
                coll.replace_one(filter, replacement)
            )?;
            format!(
                "replaceOne on {db_name}.{coll_name} matched={} modified={}",
                result.matched_count, result.modified_count
            )
        }
        ShellWrite::DeleteOne { filter, .. } => {
            let result = in_txn!(&query.control, &mut txn, coll.delete_one(filter))?;
            format!(
                "deleteOne on {db_name}.{coll_name} deleted={}",
                result.deleted_count
            )
        }
        ShellWrite::DeleteMany { filter, .. } => {
            let result = in_txn!(&query.control, &mut txn, coll.delete_many(filter))?;
            format!(
                "deleteMany on {db_name}.{coll_name} deleted={}",
                result.deleted_count
            )
        }
        ShellWrite::FindOneAndUpdate { filter, update, .. } => {
            let result = in_txn!(
                &query.control,
                &mut txn,
                coll.find_one_and_update(filter, update)
            )?;
            return match result {
                Some(doc) => docs_result(
                    vec![doc],
                    false,
                    query.mode,
                    start,
                    format!("findOneAndUpdate on {db_name}.{coll_name}"),
                ),
                None => Ok(write_message(
                    query.mode,
                    start,
                    format!("findOneAndUpdate on {db_name}.{coll_name} matched=0"),
                )),
            };
        }
        ShellWrite::Bulk { models, .. } => {
            bulk_summary(query, &coll, &db_name, &coll_name, models, txn).await?
        }
    };
    Ok(write_message(query.mode, start, message))
}

/// Ordered bulk write via collection methods.
///
/// `Client::bulk_write` is the MongoDB 8.0 command. DocumentDB and older
/// servers reject it. One guard decision already happened before this runs.
async fn bulk_summary(
    query: &ValidatedQuery,
    coll: &mongodb::Collection<Document>,
    db_name: &str,
    coll_name: &str,
    models: Vec<BulkModel>,
    mut txn: Txn<'_>,
) -> ConnectorResult<String> {
    let mut inserted = 0u64;
    let mut matched = 0u64;
    let mut modified = 0u64;
    let mut deleted = 0u64;
    for model in models {
        match model {
            BulkModel::InsertOne(document) => {
                in_txn!(&query.control, &mut txn, coll.insert_one(document))?;
                inserted = inserted.saturating_add(1);
            }
            BulkModel::UpdateOne { filter, update } => {
                let result = in_txn!(&query.control, &mut txn, coll.update_one(filter, update))?;
                matched = matched.saturating_add(result.matched_count);
                modified = modified.saturating_add(result.modified_count);
            }
            BulkModel::UpdateMany { filter, update } => {
                let result = in_txn!(&query.control, &mut txn, coll.update_many(filter, update))?;
                matched = matched.saturating_add(result.matched_count);
                modified = modified.saturating_add(result.modified_count);
            }
            BulkModel::ReplaceOne {
                filter,
                replacement,
            } => {
                let result = in_txn!(
                    &query.control,
                    &mut txn,
                    coll.replace_one(filter, replacement)
                )?;
                matched = matched.saturating_add(result.matched_count);
                modified = modified.saturating_add(result.modified_count);
            }
            BulkModel::DeleteOne(filter) => {
                let result = in_txn!(&query.control, &mut txn, coll.delete_one(filter))?;
                deleted = deleted.saturating_add(result.deleted_count);
            }
            BulkModel::DeleteMany(filter) => {
                let result = in_txn!(&query.control, &mut txn, coll.delete_many(filter))?;
                deleted = deleted.saturating_add(result.deleted_count);
            }
        }
    }
    Ok(format!(
        "bulkWrite on {db_name}.{coll_name} inserted={inserted} matched={matched} modified={modified} deleted={deleted}"
    ))
}

type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Box a session-bound driver call. Its future otherwise trips rustc's
/// higher-ranked `Send` check inside `async_trait`.
fn boxed<'a, F>(action: F) -> Boxed<'a, F::Output>
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
            _ = tokio::time::sleep(Duration::from_millis(mystique_connector_api::INACTIVITY_TIMEOUT_MS)), if saw_doc => {
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
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use mystique_connector_api::{clamp_timeout_ms, QueryControl, MAX_TIMEOUT_MS};

    use super::{find_max_time, listing, not_in_transaction, Listing};

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
            find_max_time(clamp_timeout_ms(Some(120_000))),
            Duration::from_millis(MAX_TIMEOUT_MS)
        );
        assert_eq!(
            find_max_time(clamp_timeout_ms(Some(9_000_000))),
            Duration::from_millis(MAX_TIMEOUT_MS)
        );

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
