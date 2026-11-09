//! Multi-statement execution. Plugins run a [`ValidatedBatch`] on one connection.
//!
//! Progress is a std callback that the IPC layer adapts to a Tauri channel.
//! [`ProgressPump`] rate-limits it to one event per 100 ms. Core does not
//! depend on `mystique-sql`; [`gate_sql_batch`] is the mapped plan.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::cancel::QueryControl;
use crate::error::{position_at, ConnectorError, ConnectorResult};
use crate::query::{QueryMode, QueryResult};
use crate::SqlDialect;
use crate::WriteRisk;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Risk copied from the SQL planner. Session is not a data write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum StatementKind {
    Read,
    Session,
    Write,
    Destructive,
}

impl StatementKind {
    /// Session (`SET`) does not prompt. Read mode still rejects it earlier.
    pub fn write_risk(self) -> WriteRisk {
        match self {
            Self::Read | Self::Session => WriteRisk::Read,
            Self::Write => WriteRisk::Write,
            Self::Destructive => WriteRisk::Destructive,
        }
    }
}

/// Byte range of one statement in the submitted batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SqlRange {
    pub start: u32,
    pub end: u32,
}

/// One planned statement handed to a plugin. The text is already split.
#[derive(Debug, Clone)]
pub struct PlannedStatement {
    pub index: u32,
    pub sql: String,
    pub sql_range: SqlRange,
    pub kind: StatementKind,
    pub returns_rows: bool,
    pub returns_count: bool,
    pub cursor: bool,
    pub multi_set: bool,
    pub txn_boundary: bool,
    pub explain_executes: bool,
    /// Bound values for this statement, in driver order. Never interpolated.
    pub params: Vec<serde_json::Value>,
}

/// SQL batch after the read-mode and transaction gates.
#[derive(Debug, Clone)]
pub struct SqlBatchPlan {
    pub statements: Vec<PlannedStatement>,
    pub explain_executes: bool,
    pub destructive_name: Option<String>,
    pub worst: StatementKind,
}

/// Where a progress event sits in the batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum BatchPhase {
    Started,
    Finished,
}

/// Statement-boundary progress. Not emitted once per row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BatchEvent {
    /// 0-based statement index.
    pub index: u32,
    pub total: u32,
    pub phase: BatchPhase,
}

/// Rate-limits progress to at most one event per [`PROGRESS_INTERVAL`].
///
/// The first event goes out at once. Later events inside the window replace
/// one pending event, which a timer sends when the window ends, so a long
/// statement that started inside the window is still reported. A batch of
/// thousands of fast statements costs about ten IPC events a second, not two
/// per statement. [`ProgressPump::finish`] drops anything pending: the batch
/// result supersedes it.
#[derive(Clone)]
pub struct ProgressPump {
    inner: Arc<PumpInner>,
}

struct PumpInner {
    sink: Box<dyn Fn(BatchEvent) + Send + Sync>,
    interval: Duration,
    state: Mutex<PumpState>,
}

struct PumpState {
    last_sent: Option<Instant>,
    pending: Option<BatchEvent>,
    timer_armed: bool,
    finished: bool,
}

impl ProgressPump {
    pub fn new(sink: impl Fn(BatchEvent) + Send + Sync + 'static) -> Self {
        Self::with_interval(PROGRESS_INTERVAL, sink)
    }

    fn with_interval(
        interval: Duration,
        sink: impl Fn(BatchEvent) + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: Arc::new(PumpInner {
                sink: Box::new(sink),
                interval,
                state: Mutex::new(PumpState {
                    last_sent: None,
                    pending: None,
                    timer_armed: false,
                    finished: false,
                }),
            }),
        }
    }

    pub fn emit(&self, event: BatchEvent) {
        let mut state = self.inner.lock();
        if state.finished {
            return;
        }
        let now = Instant::now();
        let wait = state
            .last_sent
            .map(|sent| self.inner.interval.saturating_sub(now.duration_since(sent)))
            .unwrap_or_default();
        if wait.is_zero() {
            state.pending = None;
            state.last_sent = Some(now);
            (self.inner.sink)(event);
            return;
        }
        state.pending = Some(event);
        if state.timer_armed {
            return;
        }
        // Without a runtime the pending event waits for the next `emit`.
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        state.timer_armed = true;
        let inner = Arc::clone(&self.inner);
        handle.spawn(async move {
            tokio::time::sleep(wait).await;
            let mut state = inner.lock();
            state.timer_armed = false;
            if state.finished {
                return;
            }
            if let Some(pending) = state.pending.take() {
                state.last_sent = Some(Instant::now());
                (inner.sink)(pending);
            }
        });
    }

    /// The batch is over. Pending and later events are dropped.
    pub fn finish(&self) {
        let mut state = self.inner.lock();
        state.finished = true;
        state.pending = None;
    }
}

impl PumpInner {
    fn lock(&self) -> std::sync::MutexGuard<'_, PumpState> {
        self.state.lock().unwrap_or_else(|err| err.into_inner())
    }
}

/// Statement-boundary progress, rate-limited by the pump.
pub fn emit_progress(progress: &Option<ProgressPump>, index: u32, total: u32, phase: BatchPhase) {
    let Some(progress) = progress else {
        return;
    };
    progress.emit(BatchEvent {
        index,
        total,
        phase,
    });
}

/// End of a batch: drop the pump's pending event.
pub fn finish_progress(progress: &Option<ProgressPump>) {
    if let Some(progress) = progress {
        progress.finish();
    }
}

/// Validated payload for [`crate::ConnectorSession::execute_batch`].
///
/// `execute_query` must not call `execute_batch`. The default batch impl loops
/// `execute_query`, and that loop would recurse.
#[derive(Clone)]
pub struct ValidatedBatch {
    pub statements: Vec<PlannedStatement>,
    pub mode: QueryMode,
    pub max_rows: u32,
    pub timeout_ms: u64,
    pub explain_executes: bool,
    pub collection: Option<String>,
    pub database: Option<String>,
    pub control: Arc<QueryControl>,
    pub progress: Option<ProgressPump>,
}

/// One statement outcome. `index` is 0-based. Extra MySQL `CALL` sets share
/// `index` and increment `set_index`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct StatementResult {
    pub index: u32,
    #[serde(default)]
    pub set_index: u32,
    pub sql_range: SqlRange,
    pub kind: StatementKind,
    #[serde(default)]
    pub result: Option<QueryResult>,
    #[serde(default)]
    pub affected_rows: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
    /// Byte offset in the submitted batch. Absent when the engine named no character.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_position: Option<crate::SourcePosition>,
    #[serde(default)]
    pub notices: Vec<String>,
}

impl StatementResult {
    pub fn success(
        stmt: &PlannedStatement,
        set_index: u32,
        result: QueryResult,
        affected_rows: Option<u64>,
        notices: Vec<String>,
    ) -> Self {
        Self {
            index: stmt.index,
            set_index,
            sql_range: stmt.sql_range.clone(),
            kind: stmt.kind,
            result: Some(result),
            affected_rows,
            error: None,
            error_position: None,
            notices,
        }
    }

    pub fn failed(stmt: &PlannedStatement, set_index: u32, err: &crate::ConnectorError) -> Self {
        let error_position = err.local_position().map(|local| crate::SourcePosition {
            offset: stmt.sql_range.start.saturating_add(local.offset),
            line: local.line,
            column: local.column,
        });
        Self {
            index: stmt.index,
            set_index,
            sql_range: stmt.sql_range.clone(),
            kind: stmt.kind,
            result: None,
            affected_rows: None,
            error: Some(err.user_message()),
            error_position,
            notices: Vec::new(),
        }
    }
}

/// Every statement that ran, plus the 0-based index that stopped the batch.
///
/// A mid-batch statement error is `Ok` with `stopped_at` set so earlier result
/// sets stay visible. Read-mode rejection is a [`ConnectorError`] instead:
/// nothing ran, so `results` would be empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct QueryBatchResult {
    pub results: Vec<StatementResult>,
    #[serde(default)]
    pub stopped_at: Option<u32>,
}

/// "1 row affected." / "N rows affected."
pub fn affected_message(rows: u64) -> String {
    if rows == 1 {
        "1 row affected.".to_string()
    } else {
        format!("{rows} rows affected.")
    }
}

/// Plan `text` once, then refuse transaction control and, in read mode, anything
/// that is not a read. The returned plan is what the plugin executes.
pub fn gate_sql_batch(
    text: &str,
    dialect: SqlDialect,
    mode: QueryMode,
) -> ConnectorResult<SqlBatchPlan> {
    gate_sql_batch_in(text, dialect, mode, false, &[])
}

/// `allow_txn` is set only for a pinned tab. Auto-commit still refuses
/// `BEGIN` / `COMMIT` / `ROLLBACK` so a transaction cannot return to the pool.
pub fn gate_sql_batch_in(
    text: &str,
    dialect: SqlDialect,
    mode: QueryMode,
    allow_txn: bool,
    params: &[serde_json::Value],
) -> ConnectorResult<SqlBatchPlan> {
    let script = mystique_sql::ScriptParams::scan(text, dialect).map_err(ConnectorError::Query)?;
    if let Err(message) = script.check_count(params.len()) {
        return Err(ConnectorError::Query(message));
    }
    // Placeholders become a same-length literal so `:id` still parses as one
    // statement. The value never enters this copy.
    let masked = mystique_sql::mask_placeholders(text, dialect);
    // Read mode needs a parse to prove a read. Write mode sends what sqlparser
    // cannot read (VACUUM, vendor syntax) to the engine as an opaque write,
    // after the write guard.
    if mode == QueryMode::Read {
        if let Some(fault) = mystique_sql::first_syntax_fault(&masked, dialect) {
            return Err(ConnectorError::query_at(
                fault.message,
                position_at(text, fault.offset),
                Some(fault.index),
            ));
        }
    }
    let planned = mystique_sql::plan_batch(&masked, dialect);
    if planned.is_empty() {
        return Err(ConnectorError::Policy("Query is empty.".into()));
    }

    let mut names = Vec::new();
    let mut read_safe = Vec::with_capacity(planned.len());
    let mut statements = Vec::with_capacity(planned.len());
    for stmt in planned {
        names.extend(stmt.destructive_names.iter().cloned());
        read_safe.push(
            stmt.risk == mystique_sql::StatementRisk::Read
                || (stmt.risk == mystique_sql::StatementRisk::Session
                    && mystique_sql::is_read_safe_set(&stmt.sql, dialect)),
        );
        let start = usize::try_from(stmt.start).unwrap_or(0);
        let end = usize::try_from(stmt.end).unwrap_or(0);
        let (sql, indexes) = script
            .statement(text, start, end, dialect)
            .map_err(ConnectorError::Query)?;
        let mut bound = Vec::with_capacity(indexes.len());
        for index in indexes {
            let Some(value) = params.get(index) else {
                return Err(ConnectorError::Query("Missing parameter value.".into()));
            };
            bound.push(value.clone());
        }
        statements.push(PlannedStatement {
            index: stmt.index,
            sql,
            params: bound,
            sql_range: SqlRange {
                start: stmt.start,
                end: stmt.end,
            },
            kind: match stmt.risk {
                mystique_sql::StatementRisk::Read => StatementKind::Read,
                mystique_sql::StatementRisk::Session => StatementKind::Session,
                mystique_sql::StatementRisk::Write => StatementKind::Write,
                mystique_sql::StatementRisk::Destructive => StatementKind::Destructive,
            },
            returns_rows: stmt.returns_rows,
            returns_count: stmt.returns_count,
            cursor: stmt.cursor,
            multi_set: stmt.multi_set,
            txn_boundary: stmt.txn_boundary,
            explain_executes: stmt.explain_executes,
        });
    }

    if !allow_txn {
        if let Some(stmt) = statements.iter().find(|stmt| stmt.txn_boundary) {
            return Err(ConnectorError::query_at(
                format!(
                    "Statement {} starts or ends a transaction ({}). Nothing ran.",
                    display_index(stmt.index),
                    sql_snippet(&stmt.sql)
                ),
                position_at(text, stmt.sql_range.start),
                Some(stmt.index),
            ));
        }
    }
    // Read mode runs reads and harmless session `SET` (search_path, time zone).
    if mode == QueryMode::Read {
        if let Some(stmt) = statements
            .iter()
            .zip(&read_safe)
            .find(|(_, safe)| !**safe)
            .map(|(stmt, _)| stmt)
        {
            return Err(ConnectorError::query_at(
                format!(
                    "policy: Statement {} is not a read ({}). Nothing ran.",
                    display_index(stmt.index),
                    sql_snippet(&stmt.sql)
                ),
                position_at(text, stmt.sql_range.start),
                Some(stmt.index),
            ));
        }
    }

    let worst = statements
        .iter()
        .map(|stmt| stmt.kind)
        .max()
        .unwrap_or(StatementKind::Read);
    let explain_executes = statements.iter().any(|stmt| stmt.explain_executes);
    let destructive_name = if names.is_empty() {
        None
    } else {
        Some(names.join(", "))
    };
    Ok(SqlBatchPlan {
        statements,
        explain_executes,
        destructive_name,
        worst,
    })
}

/// Driver errors point into the statement the driver got, where `:name`
/// became `$1` or `?`. Move each back onto the text the user typed, then set
/// its line and column.
pub fn place_error_positions(text: &str, dialect: SqlDialect, batch: &mut QueryBatchResult) {
    let script = mystique_sql::ScriptParams::scan(text, dialect).ok();
    for stmt in &mut batch.results {
        let Some(position) = stmt.error_position.as_mut() else {
            continue;
        };
        let start = usize::try_from(stmt.sql_range.start).unwrap_or(0);
        let end = usize::try_from(stmt.sql_range.end).unwrap_or(0);
        let mut offset = usize::try_from(position.offset).unwrap_or(usize::MAX);
        if let (Some(script), Some(local)) = (&script, offset.checked_sub(start)) {
            offset = start + script.source_offset(text, start, end, dialect, local);
        }
        *position = position_at(text, u32::try_from(offset).unwrap_or(u32::MAX));
    }
}

fn display_index(index: u32) -> u64 {
    u64::from(index) + 1
}

fn sql_snippet(sql: &str) -> String {
    let mut out = String::new();
    for word in sql.split_whitespace() {
        if out.is_empty() {
            if word.chars().count() > 80 {
                let cut: String = word.chars().take(79).collect();
                return format!("{cut}…");
            }
            out.push_str(word);
            continue;
        }
        let extra = word.chars().count() + 1;
        if out.chars().count() + extra > 80 {
            out.push('…');
            return out;
        }
        out.push(' ');
        out.push_str(word);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_mode_names_the_write_and_runs_nothing() {
        let err = gate_sql_batch(
            "SELECT 1; DELETE FROM t",
            SqlDialect::Postgres,
            QueryMode::Read,
        )
        .expect_err("write");
        let msg = err.to_string();
        assert!(msg.contains("Statement 2"), "{msg}");
        assert!(msg.contains("DELETE FROM t"), "{msg}");
        assert!(msg.contains("Nothing ran"), "{msg}");
        assert!(msg.starts_with("policy:"), "{msg}");
    }

    #[test]
    fn write_mode_allows_the_delete_and_refuses_begin() {
        let plan = gate_sql_batch(
            "SELECT 1; DELETE FROM t",
            SqlDialect::Postgres,
            QueryMode::Write,
        )
        .expect("write mode");
        assert_eq!(plan.statements.len(), 2);
        assert_eq!(plan.worst, StatementKind::Write);
        assert!(plan.destructive_name.is_none());

        let txn = gate_sql_batch(
            "BEGIN; UPDATE t SET a = 1",
            SqlDialect::Postgres,
            QueryMode::Write,
        )
        .expect_err("begin");
        let msg = txn.to_string();
        assert!(msg.contains("Statement 1"), "{msg}");
        assert!(msg.contains("Nothing ran"), "{msg}");
        assert!(msg.to_ascii_lowercase().contains("transaction"), "{msg}");
    }

    #[test]
    fn unparsed_statement_is_an_opaque_write_in_write_mode_only() {
        let plan = gate_sql_batch("SELECT 1; VACUUM t", SqlDialect::Postgres, QueryMode::Write)
            .expect("write mode sends it to the engine");
        assert_eq!(plan.worst, StatementKind::Write);
        let vacuum = &plan.statements[1];
        assert_eq!(vacuum.kind, StatementKind::Write);
        assert!(!vacuum.cursor);
        assert_eq!(vacuum.sql, "VACUUM t");

        let err = gate_sql_batch("SELECT 1; VACUUM t", SqlDialect::Postgres, QueryMode::Read)
            .expect_err("read mode cannot prove a read");
        let ConnectorError::QueryAt {
            statement_index,
            position,
            ..
        } = &err
        else {
            panic!("{err}");
        };
        assert_eq!(*statement_index, Some(1));
        assert_eq!(position.offset, "SELECT 1; ".len() as u32);
    }

    #[test]
    fn unparsed_transaction_control_is_still_refused_on_auto_commit() {
        for (sql, dialect) in [
            ("ABORT", SqlDialect::Postgres),
            ("-- note\nPREPARE TRANSACTION 'x'", SqlDialect::Postgres),
            ("COMMIT PREPARED 'x'", SqlDialect::Postgres),
            (
                "START TRANSACTION WITH CONSISTENT SNAPSHOT",
                SqlDialect::Mysql,
            ),
            ("XA START 'x'", SqlDialect::Mysql),
        ] {
            let err = gate_sql_batch(sql, dialect, QueryMode::Write).expect_err(sql);
            assert!(err.to_string().contains("transaction"), "{sql}: {err}");
            let pinned = gate_sql_batch_in(sql, dialect, QueryMode::Write, true, &[])
                .unwrap_or_else(|err| panic!("{sql}: {err}"));
            assert!(pinned.statements[0].txn_boundary, "{sql}");
        }
        let vacuum =
            gate_sql_batch("VACUUM", SqlDialect::Postgres, QueryMode::Write).expect("vacuum");
        assert!(!vacuum.statements[0].txn_boundary);
    }

    #[test]
    fn read_mode_runs_a_safe_set_and_refuses_the_rest() {
        let plan = gate_sql_batch(
            "SET search_path TO public; SELECT 1",
            SqlDialect::Postgres,
            QueryMode::Read,
        )
        .expect("safe set");
        assert_eq!(plan.statements[0].kind, StatementKind::Session);
        for sql in [
            "SET TRANSACTION READ WRITE",
            "SET statement_timeout = 0",
            "SET ROLE admin",
            "SET work_mem = '1GB'",
        ] {
            let err = gate_sql_batch(sql, SqlDialect::Postgres, QueryMode::Read).expect_err(sql);
            assert!(err.to_string().starts_with("policy:"), "{err}");
        }
    }

    #[test]
    fn driver_error_offsets_land_on_the_typed_text() {
        for dialect in [SqlDialect::Postgres, SqlDialect::Mysql] {
            let text = "SELECT 1;\nSELECT :long_name, nosuchcol";
            let plan = gate_sql_batch_in(text, dialect, QueryMode::Read, false, &[1.into()])
                .expect("plan");
            let stmt = plan.statements.last().expect("statement");
            // What the driver reports: an offset in the SQL it received.
            let local = stmt.sql.find("nosuchcol").expect("driver sql");
            let err = ConnectorError::query_at(
                "column does not exist",
                crate::SourcePosition {
                    offset: u32::try_from(local).expect("offset"),
                    line: 1,
                    column: 1,
                },
                None,
            );
            let mut batch = QueryBatchResult {
                results: vec![StatementResult::failed(stmt, 0, &err)],
                stopped_at: Some(stmt.index),
            };
            place_error_positions(text, dialect, &mut batch);
            let position = batch.results[0].error_position.clone().expect("position");
            assert_eq!(
                text.get(position.offset as usize..),
                Some("nosuchcol"),
                "{dialect:?}"
            );
            assert_eq!((position.line, position.column), (2, 20), "{dialect:?}");
        }
    }

    fn recording_pump(interval: Duration) -> (ProgressPump, Arc<Mutex<Vec<BatchEvent>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let pump = ProgressPump::with_interval(interval, {
            let seen = Arc::clone(&seen);
            move |event| {
                seen.lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .push(event);
            }
        });
        (pump, seen)
    }

    fn started(index: u32) -> BatchEvent {
        BatchEvent {
            index,
            total: 1_000,
            phase: BatchPhase::Started,
        }
    }

    #[tokio::test]
    async fn pump_sends_at_most_one_event_per_window() {
        let (pump, seen) = recording_pump(Duration::from_millis(100));
        for index in 0..1_000 {
            pump.emit(started(index));
            pump.emit(BatchEvent {
                phase: BatchPhase::Finished,
                ..started(index)
            });
        }
        // The first event goes out at once; the rest wait for the window.
        assert_eq!(seen.lock().unwrap_or_else(|err| err.into_inner()).len(), 1);
        tokio::time::sleep(Duration::from_millis(250)).await;
        let got = seen.lock().unwrap_or_else(|err| err.into_inner()).clone();
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[0].index, 0);
        // The trailing event is the latest one, so the UI shows where the batch is.
        assert_eq!(got[1].index, 999);
        assert_eq!(got[1].phase, BatchPhase::Finished);
    }

    #[tokio::test]
    async fn a_long_statement_inside_the_window_is_still_reported() {
        let (pump, seen) = recording_pump(Duration::from_millis(40));
        pump.emit(started(0));
        pump.emit(started(1));
        // Statement 1 is still running when the window ends.
        tokio::time::sleep(Duration::from_millis(120)).await;
        let got = seen.lock().unwrap_or_else(|err| err.into_inner()).clone();
        assert_eq!(got.iter().map(|e| e.index).collect::<Vec<_>>(), vec![0, 1]);
    }

    #[tokio::test]
    async fn finish_drops_the_pending_event() {
        let (pump, seen) = recording_pump(Duration::from_millis(40));
        pump.emit(started(0));
        pump.emit(started(1));
        pump.finish();
        pump.emit(started(2));
        tokio::time::sleep(Duration::from_millis(100)).await;
        let got = seen.lock().unwrap_or_else(|err| err.into_inner()).clone();
        assert_eq!(got.len(), 1, "{got:?}");
    }

    #[test]
    fn comment_select_is_a_read_row_route() {
        let plan = gate_sql_batch("-- note\nSELECT 1", SqlDialect::Postgres, QueryMode::Read)
            .expect("comment");
        assert!(plan.statements[0].returns_rows);
        assert!(plan.statements[0].cursor);
    }
}
