//! Plugin conformance kit.
//!
//! One suite that every engine plugin runs against a live server (or a temp
//! file for SQLite). It checks the SPI contract the rest of Mystique relies
//! on, not engine features:
//!
//! - the descriptor and capabilities are coherent (declared kinds, depth);
//! - connect, ping, list, describe, page, sort, count and stream agree on the
//!   same fixture table;
//! - Read mode is enforced **by the server**: a write sent straight to the
//!   plugin in Read mode fails and changes nothing;
//! - cancel reaches the server; pinned transactions roll back and commit;
//! - every capability flag that is on has a working method behind it.
//!
//! A plugin supplies a [`Fixture`] (its config plus a few statements in its
//! own language) and calls [`run`]. Live suites are `#[ignore]`d and run by
//! the `live` CI job against docker services; see `docs/CONFORMANCE.md`.
//!
//! ```ignore
//! let report = mystique_conformance::run(&PostgresConnector, &fixture).await;
//! report.assert_passed();
//! ```

#![deny(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mystique_connector_api::{
    gate_sql_batch_in, CatalogPath, ConnectionConfig, ConnectorError, ConnectorSession,
    DatabaseConnector, DistinctColumnValuesInput, EngineCapabilities, FetchServerMetricsInput,
    FetchTablePreviewInput, ListCatalogChildrenInput, PagingStyle, QueryControl, QueryMode,
    QueryResult, RowSink, RowSource, SupportDepth, TableAdminInput, TableAdminOp, TableObjectKind,
    ValidatedBatch, ValidatedQuery, KIND_DATABASE, KIND_SCHEMA,
};
use mystique_sql::SqlDialect;
use serde_json::Value as JsonValue;

/// Name of the table or collection the kit creates.
pub const TABLE: &str = "mystique_conformance";
/// Rows the fixture table holds after setup (`id` 1..=ROWS).
pub const ROWS: u32 = 25;

/// What a plugin hands the kit.
///
/// The fixture table has a key column (`id`, or `_id` for documents) holding
/// 1..=[`ROWS`] and a text column `name`. Statements run in the engine's own
/// language, in Write mode unless the check says otherwise.
pub struct Fixture {
    pub config: ConnectionConfig,
    /// Database the table lives in, as the catalog names it.
    pub database: Option<String>,
    /// Schema, for engines that have them (`public`).
    pub schema: Option<String>,
    /// Key column: `id` for SQL, `_id` for documents.
    pub key: String,
    /// Drop and recreate the table with [`ROWS`] rows.
    pub setup: Vec<String>,
    /// Drop the table.
    pub teardown: Vec<String>,
    /// Returns one cell: the number of rows in the table.
    pub count: String,
    /// Inserts exactly one row (`id` = ROWS + 1).
    pub insert: String,
    /// Runs for at least five seconds, for the cancel check. `None` skips it.
    pub slow: Option<String>,
    /// SQL engines: plans pinned batches. `None` skips the transaction check.
    pub dialect: Option<SqlDialect>,
    /// Document engines: the collection a query runs against.
    pub collection: Option<String>,
}

impl Fixture {
    fn query(&self, text: &str, mode: QueryMode) -> ValidatedQuery {
        ValidatedQuery {
            collection: self.collection.clone(),
            database: self.database.clone(),
            ..ValidatedQuery::new(text, mode, 1_000, 30_000)
        }
    }

    fn preview(&self, max_rows: u32) -> FetchTablePreviewInput {
        FetchTablePreviewInput {
            table: TABLE.into(),
            schema: self.schema.clone(),
            database: self.database.clone(),
            max_rows,
            offset: 0,
            filters: Vec::new(),
            code_filter: None,
            sort_column: None,
            sort_dir: None,
            page_token: None,
            control: QueryControl::new(),
        }
    }
}

/// How one check ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    /// Not applicable to this engine (capability off, no fixture statement).
    Skipped(String),
    Failed(String),
}

/// One named check.
#[derive(Debug, Clone)]
pub struct Check {
    pub name: &'static str,
    pub outcome: Outcome,
    pub elapsed: Duration,
}

/// Every check, in run order.
#[derive(Debug, Default)]
pub struct Report {
    pub engine: String,
    pub checks: Vec<Check>,
}

impl Report {
    pub fn failures(&self) -> Vec<&Check> {
        self.checks
            .iter()
            .filter(|c| matches!(c.outcome, Outcome::Failed(_)))
            .collect()
    }

    pub fn passed(&self, name: &str) -> bool {
        self.checks
            .iter()
            .any(|c| c.name == name && c.outcome == Outcome::Passed)
    }

    /// Panic with the whole report when any check failed.
    #[track_caller]
    pub fn assert_passed(&self) {
        assert!(self.failures().is_empty(), "{self}");
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "conformance: {}", self.engine)?;
        for c in &self.checks {
            let (mark, note) = match &c.outcome {
                Outcome::Passed => ("ok  ", String::new()),
                Outcome::Skipped(why) => ("skip", format!(" ({why})")),
                Outcome::Failed(why) => ("FAIL", format!(": {why}")),
            };
            writeln!(
                f,
                "  {mark} {:<28} {:>6} ms{note}",
                c.name,
                c.elapsed.as_millis()
            )?;
        }
        Ok(())
    }
}

type CheckResult = Result<Outcome, String>;

fn skip(why: impl Into<String>) -> CheckResult {
    Ok(Outcome::Skipped(why.into()))
}

fn ensure(cond: bool, why: impl FnOnce() -> String) -> Result<(), String> {
    if cond {
        Ok(())
    } else {
        Err(why())
    }
}

fn err(e: ConnectorError) -> String {
    e.to_string()
}

/// Run the whole suite. Never panics; read the [`Report`].
pub async fn run(connector: &dyn DatabaseConnector, fixture: &Fixture) -> Report {
    let mut report = Report {
        engine: connector.id().to_string(),
        checks: Vec::new(),
    };

    record(&mut report, "descriptor", async { descriptor(connector) }).await;
    record(&mut report, "test_connection", async {
        connector
            .test_connection(&fixture.config)
            .await
            .map(|r| {
                if r.ok {
                    Outcome::Passed
                } else {
                    Outcome::Failed(r.message)
                }
            })
            .map_err(err)
    })
    .await;

    let session = match connector.connect(&fixture.config).await {
        Ok(s) => s,
        Err(e) => {
            report.checks.push(Check {
                name: "connect",
                outcome: Outcome::Failed(e.to_string()),
                elapsed: Duration::ZERO,
            });
            return report;
        }
    };
    let s: &dyn ConnectorSession = session.as_ref();
    let caps = s.capabilities();

    record(&mut report, "ping", async {
        ensure(s.engine_id() == connector.id(), || {
            format!("session engine `{}` ≠ `{}`", s.engine_id(), connector.id())
        })?;
        s.ping().await.map(|()| Outcome::Passed).map_err(err)
    })
    .await;

    record(&mut report, "setup", async {
        for stmt in &fixture.setup {
            s.execute_query(&fixture.query(stmt, QueryMode::Write))
                .await
                .map_err(|e| format!("{stmt}: {e}"))?;
        }
        let n = count(s, fixture).await?;
        ensure(n == i64::from(ROWS), || {
            format!("fixture has {n} rows, want {ROWS}")
        })?;
        Ok(Outcome::Passed)
    })
    .await;
    if !report.passed("setup") {
        teardown(s, fixture).await;
        let _ = s.close().await;
        return report;
    }

    record(&mut report, "list_catalog", async {
        list_catalog(s, &caps, fixture).await
    })
    .await;
    record(&mut report, "list_catalog_children", async {
        catalog_children(s, &caps, fixture).await
    })
    .await;
    record(&mut report, "describe_table", async {
        describe(s, fixture).await
    })
    .await;
    record(&mut report, "preview_pages", async {
        pages(s, &caps, fixture).await
    })
    .await;
    record(&mut report, "preview_sort", async {
        sort(s, fixture).await
    })
    .await;
    record(&mut report, "preview_count", async {
        let stats = s
            .fetch_table_preview_stats(&fixture.preview(1))
            .await
            .map_err(err)?;
        if !stats.total_known {
            return skip("engine has no exact count");
        }
        ensure(stats.total_rows == u64::from(ROWS), || {
            format!("total {} ≠ {ROWS}", stats.total_rows)
        })?;
        Ok(Outcome::Passed)
    })
    .await;
    record(&mut report, "query_read", async {
        let n = count(s, fixture).await?;
        ensure(n == i64::from(ROWS), || format!("count {n} ≠ {ROWS}"))?;
        Ok(Outcome::Passed)
    })
    .await;
    record(&mut report, "read_mode_enforced", async {
        read_mode(s, fixture).await
    })
    .await;
    record(&mut report, "stream_rows", async {
        stream(s, fixture).await
    })
    .await;
    record(&mut report, "cancel", async {
        cancel(s, &caps, fixture).await
    })
    .await;
    record(&mut report, "explain", async {
        if !caps.supports_explain_plan {
            return skip("supports_explain_plan is off");
        }
        s.explain_query(&fixture.query(&fixture.count, QueryMode::Read), false)
            .await
            .map(|_| Outcome::Passed)
            .map_err(err)
    })
    .await;
    record(&mut report, "pinned_transaction", async {
        pinned(s, &caps, fixture).await
    })
    .await;
    record(&mut report, "capability_honesty", async {
        honesty(s, &caps, fixture).await
    })
    .await;

    teardown(s, fixture).await;
    record(&mut report, "close", async {
        s.close().await.map(|()| Outcome::Passed).map_err(err)
    })
    .await;
    report
}

async fn record<F>(report: &mut Report, name: &'static str, check: F)
where
    F: std::future::Future<Output = CheckResult>,
{
    let started = Instant::now();
    // A hung check fails instead of hanging CI.
    let outcome = match tokio::time::timeout(Duration::from_secs(60), check).await {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(why)) => Outcome::Failed(why),
        Err(_) => Outcome::Failed("timed out after 60 s".into()),
    };
    report.checks.push(Check {
        name,
        outcome,
        elapsed: started.elapsed(),
    });
}

async fn teardown(s: &dyn ConnectorSession, fixture: &Fixture) {
    for stmt in &fixture.teardown {
        let _ = s
            .execute_query(&fixture.query(stmt, QueryMode::Write))
            .await;
    }
}

/// Read the fixture's count statement as one integer.
async fn count(s: &dyn ConnectorSession, fixture: &Fixture) -> Result<i64, String> {
    let result = s
        .execute_query(&fixture.query(&fixture.count, QueryMode::Read))
        .await
        .map_err(|e| format!("{}: {e}", fixture.count))?;
    first_int(&result).ok_or_else(|| format!("count returned {:?}", result.rows.first()))
}

fn first_int(result: &QueryResult) -> Option<i64> {
    let cell = result.rows.first()?.iter().find(|v| !v.is_null())?;
    as_int(cell)
}

fn as_int(cell: &JsonValue) -> Option<i64> {
    cell.as_i64()
        .or_else(|| cell.as_str().and_then(|t| t.parse().ok()))
        .or_else(|| cell.as_f64().map(|f| f as i64))
        // Mongo extended JSON: {"$numberLong": "7"}
        .or_else(|| {
            cell.as_object()?
                .values()
                .next()
                .and_then(|v| v.as_str()?.parse().ok())
        })
}

fn descriptor(connector: &dyn DatabaseConnector) -> CheckResult {
    let caps = connector.capabilities();
    ensure(caps.engine == connector.id(), || {
        format!(
            "capabilities.engine `{}` ≠ id `{}`",
            caps.engine,
            connector.id()
        )
    })?;
    ensure(!caps.object_kinds.is_empty(), || {
        "object_kinds is empty: declare what list_catalog returns".into()
    })?;
    let mut seen = BTreeSet::new();
    for k in &caps.object_kinds {
        ensure(
            k.id == k.id.to_ascii_lowercase() && !k.id.is_empty(),
            || format!("kind id `{}` must be non-empty lower case", k.id),
        )?;
        ensure(seen.insert(k.id.clone()), || {
            format!("kind `{}` declared twice", k.id)
        })?;
        ensure(!k.editable || k.browsable, || {
            format!("kind `{}` is editable but not browsable", k.id)
        })?;
    }
    ensure(caps.object_kinds.iter().any(|k| k.primary), || {
        "no primary kind: the explorer list would be empty".into()
    })?;
    if caps.support_depth != SupportDepth::Operate {
        ensure(
            !caps.supports_server_metrics && !caps.supports_er_diagram,
            || "below operate, metrics and ER must be off (apply_support_depth)".into(),
        )?;
    }
    let d = connector.descriptor();
    ensure(d.id == connector.id(), || "descriptor id mismatch".into())?;
    Ok(Outcome::Passed)
}

async fn list_catalog(
    s: &dyn ConnectorSession,
    caps: &EngineCapabilities,
    fixture: &Fixture,
) -> CheckResult {
    let dbs = s
        .list_catalog(fixture.database.as_deref(), true)
        .await
        .map_err(err)?;
    let declared: BTreeSet<&str> = caps.object_kinds.iter().map(|k| k.id.as_str()).collect();
    for db in &dbs {
        for o in &db.objects {
            ensure(declared.contains(o.kind.as_str()), || {
                format!(
                    "`{}` has kind `{}`, which object_kinds does not declare",
                    o.name, o.kind
                )
            })?;
        }
    }
    let found = dbs.iter().flat_map(|d| d.objects.iter()).find(|o| {
        o.name == TABLE
            && (fixture.schema.is_none() || o.schema.as_deref() == fixture.schema.as_deref())
    });
    let Some(table) = found else {
        return Err(format!("`{TABLE}` is not in list_catalog"));
    };
    let spec = caps.object_kinds.iter().find(|k| k.id == table.kind);
    ensure(spec.is_some_and(|k| k.browsable && k.editable), || {
        format!(
            "the fixture's kind `{}` must be browsable and editable",
            table.kind
        )
    })?;
    Ok(Outcome::Passed)
}

/// Walk root → database → (schema) → objects with a page size of 1 at each
/// level, so the cursor is followed, and find the fixture table.
async fn catalog_children(
    s: &dyn ConnectorSession,
    caps: &EngineCapabilities,
    fixture: &Fixture,
) -> CheckResult {
    let roots = all_children(s, CatalogPath::default(), None).await?;
    ensure(roots.iter().all(|n| n.kind == KIND_DATABASE), || {
        "the root level must list databases".into()
    })?;
    let database = fixture
        .database
        .clone()
        .or_else(|| roots.first().map(|n| n.name.clone()))
        .ok_or("no database at the root")?;
    ensure(roots.iter().any(|n| n.name == database), || {
        format!("database `{database}` is not at the root")
    })?;
    let mut path = CatalogPath {
        database: Some(database),
        ..CatalogPath::default()
    };
    if caps.explorer.layout == "schema-tree" {
        let schemas = all_children(s, path.clone(), None).await?;
        ensure(schemas.iter().all(|n| n.kind == KIND_SCHEMA), || {
            "a schema-tree database lists schemas".into()
        })?;
        let schema = fixture
            .schema
            .clone()
            .ok_or("schema-tree fixture needs a schema")?;
        ensure(schemas.iter().any(|n| n.name == schema), || {
            format!("schema `{schema}` is not listed")
        })?;
        path.schema = Some(schema);
    }
    let objects = all_children(s, path.clone(), None).await?;
    ensure(objects.iter().any(|n| n.name == TABLE), || {
        format!("`{TABLE}` is not among {} children", objects.len())
    })?;
    // A filter that matches only the fixture.
    let filtered = all_children(s, path, Some(TABLE.to_uppercase())).await?;
    ensure(filtered.iter().any(|n| n.name == TABLE), || {
        "the name filter (case-insensitive) lost the fixture".into()
    })?;
    Ok(Outcome::Passed)
}

async fn all_children(
    s: &dyn ConnectorSession,
    parent: CatalogPath,
    filter: Option<String>,
) -> Result<Vec<mystique_connector_api::CatalogNode>, String> {
    let mut out = Vec::new();
    let mut cursor = None;
    for _ in 0..10_000 {
        let page = s
            .list_catalog_children(&ListCatalogChildrenInput {
                parent: parent.clone(),
                cursor: cursor.take(),
                limit: 1,
                filter: filter.clone(),
            })
            .await
            .map_err(err)?;
        ensure(page.nodes.len() <= 1, || "a page exceeded its limit".into())?;
        out.extend(page.nodes);
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => return Ok(out),
        }
    }
    Err("the catalog cursor never ended".into())
}

async fn describe(s: &dyn ConnectorSession, fixture: &Fixture) -> CheckResult {
    let cols = s
        .describe_table(
            fixture.database.as_deref(),
            fixture.schema.as_deref(),
            TABLE,
        )
        .await
        .map_err(err)?;
    let key = cols
        .iter()
        .find(|c| c.name == fixture.key)
        .ok_or_else(|| format!("no `{}` column", fixture.key))?;
    ensure(key.pk, || format!("`{}` is not marked pk", fixture.key))?;
    ensure(!key.data_type.trim().is_empty(), || {
        "the key has no type label".into()
    })?;
    ensure(cols.iter().any(|c| c.name == "name"), || {
        "no `name` column".into()
    })?;
    Ok(Outcome::Passed)
}

/// Every row exactly once, seven at a time, by offset or token.
async fn pages(
    s: &dyn ConnectorSession,
    caps: &EngineCapabilities,
    fixture: &Fixture,
) -> CheckResult {
    const PAGE: u32 = 7;
    let mut keys = BTreeSet::new();
    let mut seen = 0usize;
    let mut input = fixture.preview(PAGE);
    for _ in 0..100 {
        let page = s.fetch_table_preview(&input).await.map_err(err)?;
        ensure(page.rows.len() <= PAGE as usize, || {
            format!("page of {} rows over max_rows {PAGE}", page.rows.len())
        })?;
        let idx = page
            .columns
            .iter()
            .position(|c| c.name == fixture.key)
            .ok_or_else(|| format!("preview has no `{}` column", fixture.key))?;
        for row in &page.rows {
            let k = row
                .get(idx)
                .and_then(as_int)
                .ok_or_else(|| format!("key cell {:?}", row.get(idx)))?;
            keys.insert(k);
            seen += 1;
        }
        match caps.paging {
            PagingStyle::Token => match page.next_page_token {
                Some(t) => input.page_token = Some(t),
                None => break,
            },
            PagingStyle::Offset => {
                if page.rows.len() < PAGE as usize {
                    break;
                }
                input.offset += PAGE;
            }
        }
    }
    ensure(seen == ROWS as usize, || {
        format!("saw {seen} rows, want {ROWS}")
    })?;
    ensure(keys.len() == ROWS as usize, || {
        format!(
            "{} distinct keys over {seen} rows: pages overlap",
            keys.len()
        )
    })?;
    Ok(Outcome::Passed)
}

async fn sort(s: &dyn ConnectorSession, fixture: &Fixture) -> CheckResult {
    let mut input = fixture.preview(1);
    input.sort_column = Some(fixture.key.clone());
    input.sort_dir = Some("desc".into());
    let page = s.fetch_table_preview(&input).await.map_err(err)?;
    let idx = page
        .columns
        .iter()
        .position(|c| c.name == fixture.key)
        .ok_or("no key column")?;
    let top = page
        .rows
        .first()
        .and_then(|r| r.get(idx))
        .and_then(as_int)
        .ok_or("no row")?;
    ensure(top == i64::from(ROWS), || {
        format!("descending sort starts at {top}, want {ROWS}")
    })?;
    Ok(Outcome::Passed)
}

/// The plugin, not the classifier, must refuse a write in Read mode.
async fn read_mode(s: &dyn ConnectorSession, fixture: &Fixture) -> CheckResult {
    let attempt = s
        .execute_query(&fixture.query(&fixture.insert, QueryMode::Read))
        .await;
    let after = count(s, fixture).await?;
    ensure(after == i64::from(ROWS), || {
        format!("a Read-mode insert changed the table ({after} rows)")
    })?;
    ensure(attempt.is_err(), || {
        "a Read-mode insert returned Ok without writing; it must fail".into()
    })?;
    Ok(Outcome::Passed)
}

#[derive(Default)]
struct Collect {
    columns: usize,
    rows: usize,
}

#[async_trait::async_trait]
impl RowSink for Collect {
    async fn columns(
        &mut self,
        columns: Vec<mystique_connector_api::ColumnInfo>,
    ) -> mystique_connector_api::ConnectorResult<()> {
        self.columns = columns.len();
        Ok(())
    }

    async fn rows(
        &mut self,
        rows: Vec<Vec<JsonValue>>,
    ) -> mystique_connector_api::ConnectorResult<()> {
        self.rows += rows.len();
        Ok(())
    }
}

async fn stream(s: &dyn ConnectorSession, fixture: &Fixture) -> CheckResult {
    let mut sink = Collect::default();
    s.stream_rows(
        &RowSource::Table {
            database: fixture.database.clone(),
            schema: fixture.schema.clone(),
            table: TABLE.into(),
        },
        &QueryControl::new(),
        &mut sink,
    )
    .await
    .map_err(err)?;
    ensure(sink.columns >= 2, || {
        format!("{} columns streamed", sink.columns)
    })?;
    ensure(sink.rows == ROWS as usize, || {
        format!("{} rows streamed, want {ROWS}", sink.rows)
    })?;
    Ok(Outcome::Passed)
}

async fn cancel(
    s: &dyn ConnectorSession,
    caps: &EngineCapabilities,
    fixture: &Fixture,
) -> CheckResult {
    if !caps.supports_cancel_query {
        return skip("supports_cancel_query is off");
    }
    let Some(slow) = fixture.slow.as_deref() else {
        return skip("fixture has no slow statement");
    };
    let control = QueryControl::new();
    let query = ValidatedQuery {
        control: Arc::clone(&control),
        ..fixture.query(slow, QueryMode::Read)
    };
    let started = Instant::now();
    let (result, ()) = tokio::join!(s.execute_query(&query), async {
        tokio::time::sleep(Duration::from_millis(400)).await;
        control.cancel();
    });
    let took = started.elapsed();
    ensure(result.is_err(), || {
        "the slow statement finished despite cancel".into()
    })?;
    ensure(took < Duration::from_secs(4), || {
        format!("cancel took {} ms to stop the statement", took.as_millis())
    })?;
    // The session is still usable afterwards.
    s.ping()
        .await
        .map_err(|e| format!("ping after cancel: {e}"))?;
    Ok(Outcome::Passed)
}

fn batch(fixture: &Fixture, dialect: SqlDialect, sql: &str) -> Result<ValidatedBatch, String> {
    let plan = gate_sql_batch_in(sql, dialect, QueryMode::Write, true, &[]).map_err(err)?;
    Ok(ValidatedBatch {
        statements: plan.statements,
        mode: QueryMode::Write,
        max_rows: 100,
        timeout_ms: 30_000,
        explain_executes: plan.explain_executes,
        collection: fixture.collection.clone(),
        database: fixture.database.clone(),
        control: QueryControl::new(),
        progress: None,
    })
}

/// Rollback discards the insert; commit keeps it (then the row is removed by
/// a fresh setup in later runs).
async fn pinned(
    s: &dyn ConnectorSession,
    caps: &EngineCapabilities,
    fixture: &Fixture,
) -> CheckResult {
    if !caps.supports_transactions {
        return skip("supports_transactions is off");
    }
    let Some(dialect) = fixture.dialect else {
        return skip("fixture has no SQL dialect for batches");
    };
    let mut pin = s.begin_pinned().await.map_err(err)?;
    let insert = batch(fixture, dialect, &fixture.insert)?;
    let first = pin.execute(&insert).await.map_err(err)?;
    if let Some(e) = first.results.iter().find_map(|r| r.error.as_ref()) {
        return Err(format!("insert in the pinned session failed: {e:?}"));
    }
    ensure(pin.in_transaction(), || {
        "a write on a pinned session must open a transaction".into()
    })?;
    // Another connection does not see the uncommitted row.
    let outside = count(s, fixture).await?;
    ensure(outside == i64::from(ROWS), || {
        format!("uncommitted insert visible outside the pin ({outside})")
    })?;
    pin.rollback().await.map_err(err)?;
    ensure(!pin.in_transaction(), || {
        "still in a transaction after rollback".into()
    })?;
    let after_rollback = count(s, fixture).await?;
    ensure(after_rollback == i64::from(ROWS), || {
        format!("rollback kept the insert ({after_rollback})")
    })?;

    let again = pin.execute(&insert).await.map_err(err)?;
    if let Some(e) = again.results.iter().find_map(|r| r.error.as_ref()) {
        return Err(format!("second insert failed: {e:?}"));
    }
    pin.commit().await.map_err(err)?;
    drop(pin);
    let after_commit = count(s, fixture).await?;
    ensure(after_commit == i64::from(ROWS) + 1, || {
        format!("commit lost the insert ({after_commit})")
    })?;
    Ok(Outcome::Passed)
}

/// A flag that is on must have a method behind it that does not answer
/// NotSupported.
async fn honesty(
    s: &dyn ConnectorSession,
    caps: &EngineCapabilities,
    fixture: &Fixture,
) -> CheckResult {
    let mut problems = Vec::new();
    let unsupported = |r: &mystique_connector_api::ConnectorResult<_>| {
        matches!(r, Err(ConnectorError::NotSupported(_)))
    };
    if caps.supports_distinct_values {
        let r = s
            .distinct_column_values(&DistinctColumnValuesInput {
                table: TABLE.into(),
                schema: fixture.schema.clone(),
                database: fixture.database.clone(),
                column: "name".into(),
                limit: 10,
                prefix: None,
            })
            .await;
        if let Err(e) = &r {
            problems.push(format!("distinct values: {e}"));
        }
    }
    if caps.table_admin.create_statement {
        let r = s
            .apply_table_admin(&TableAdminInput {
                op: TableAdminOp::CreateStatement,
                name: TABLE.into(),
                schema: fixture.schema.clone(),
                database: fixture.database.clone(),
                kind: TableObjectKind::Table,
                object_kind: caps
                    .object_kinds
                    .iter()
                    .find(|k| k.primary && k.editable)
                    .map_or_else(|| "table".into(), |k| k.id.clone()),
                new_name: None,
            })
            .await;
        match r {
            Ok(res)
                if res
                    .statement
                    .as_deref()
                    .is_some_and(|t| !t.trim().is_empty()) => {}
            Ok(_) => problems.push("create statement came back empty".into()),
            Err(e) => problems.push(format!("create statement: {e}")),
        }
    }
    if caps.table_admin.view_structure {
        let r = s
            .describe_object_structure(
                fixture.database.as_deref(),
                fixture.schema.as_deref(),
                TABLE,
                None,
            )
            .await;
        if unsupported(&r) {
            problems.push("view_structure is on but describe_object_structure is not".into());
        }
    }
    if caps.supports_server_metrics {
        let r = s
            .fetch_server_metrics(&FetchServerMetricsInput {
                include_sessions: true,
                include_slow_queries: true,
                slow_query_limit: 5,
                min_duration_ms: 0,
            })
            .await;
        if let Err(e) = &r {
            problems.push(format!("server metrics: {e}"));
        }
    }
    if caps.supports_structure_edit {
        let r = s
            .describe_table_definition(
                fixture.database.as_deref(),
                fixture.schema.as_deref(),
                TABLE,
                None,
            )
            .await;
        if let Err(e) = &r {
            problems.push(format!("table definition: {e}"));
        }
    }
    if caps.supports_er_diagram {
        let r = s.list_relationships(fixture.database.as_deref()).await;
        if let Err(e) = &r {
            problems.push(format!("relationships: {e}"));
        }
    }
    if problems.is_empty() {
        Ok(Outcome::Passed)
    } else {
        Err(problems.join("; "))
    }
}

/// `std::env::var(key)` or `default`. Live suites read their server from
/// `MYSTIQUE_<ENGINE>_*` so the CI job and a laptop can differ.
pub fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_read_from_numbers_strings_and_extended_json() {
        assert_eq!(as_int(&serde_json::json!(7)), Some(7));
        assert_eq!(as_int(&serde_json::json!("7")), Some(7));
        assert_eq!(as_int(&serde_json::json!({"$numberLong": "7"})), Some(7));
        assert_eq!(as_int(&serde_json::json!(null)), None);
    }

    #[test]
    fn a_report_lists_failures_and_formats() {
        let report = Report {
            engine: "x".into(),
            checks: vec![
                Check {
                    name: "a",
                    outcome: Outcome::Passed,
                    elapsed: Duration::ZERO,
                },
                Check {
                    name: "b",
                    outcome: Outcome::Failed("boom".into()),
                    elapsed: Duration::ZERO,
                },
            ],
        };
        assert_eq!(report.failures().len(), 1);
        assert!(report.passed("a"));
        assert!(report.to_string().contains("FAIL b"));
    }
}
