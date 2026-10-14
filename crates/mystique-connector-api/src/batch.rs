//! Multi-statement execution. Plugins run a [`ValidatedBatch`] on one connection.
//!
//! Progress is a std callback. The IPC layer adapts it to a Tauri channel and
//! coalesces repeats. Core does not depend on `mystique-sql`; [`gate_sql_batch`]
//! is the mapped plan.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::cancel::QueryControl;
use crate::error::{ConnectorError, ConnectorResult};
use crate::query::{QueryMode, QueryResult};
use crate::SqlDialect;
use crate::WriteRisk;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Risk copied from the SQL planner. Session is not a data write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
#[serde(rename_all = "camelCase")]
pub enum BatchPhase {
    Started,
    Finished,
}

/// Statement-boundary progress. Not emitted once per row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchEvent {
    /// 0-based statement index.
    pub index: u32,
    pub total: u32,
    pub phase: BatchPhase,
}

/// Coalesces repeat progress events to [`PROGRESS_INTERVAL`].
///
/// A change of statement index and any `force` event (statement finished, flush)
/// are delivered immediately. Repeats of the same statement inside the window
/// collapse to the latest one, which [`ProgressPump::flush`] sends.
#[derive(Clone)]
pub struct ProgressPump {
    inner: Arc<PumpInner>,
}

struct PumpInner {
    sink: Box<dyn Fn(BatchEvent) + Send + Sync>,
    state: Mutex<PumpState>,
}

struct PumpState {
    last_sent: Option<Instant>,
    last_index: Option<u32>,
    pending: Option<BatchEvent>,
}

impl ProgressPump {
    pub fn new(sink: impl Fn(BatchEvent) + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(PumpInner {
                sink: Box::new(sink),
                state: Mutex::new(PumpState {
                    last_sent: None,
                    last_index: None,
                    pending: None,
                }),
            }),
        }
    }

    pub fn emit(&self, event: BatchEvent, force: bool) {
        let mut state = self.lock();
        let now = Instant::now();
        let index_changed = state.last_index != Some(event.index);
        let due = state
            .last_sent
            .is_none_or(|sent| now.duration_since(sent) >= PROGRESS_INTERVAL);
        if force || index_changed || due {
            // `pending` is a repeat of the statement already delivered. A new
            // statement replaces it. `flush` still sends a repeat that never
            // got a later event.
            state.pending = None;
            state.last_sent = Some(now);
            state.last_index = Some(event.index);
            (self.inner.sink)(event);
        } else {
            state.pending = Some(event);
        }
    }

    /// Sends a coalesced event that was still inside the window.
    pub fn flush(&self) {
        let mut state = self.lock();
        if let Some(pending) = state.pending.take() {
            state.last_sent = Some(Instant::now());
            state.last_index = Some(pending.index);
            (self.inner.sink)(pending);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, PumpState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }
}

/// Statement-boundary progress. Finished events are not coalesced.
pub fn emit_progress(progress: &Option<ProgressPump>, index: u32, total: u32, phase: BatchPhase) {
    let Some(progress) = progress else {
        return;
    };
    let force = matches!(phase, BatchPhase::Finished);
    progress.emit(
        BatchEvent {
            index,
            total,
            phase,
        },
        force,
    );
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
            notices,
        }
    }

    pub fn failed(stmt: &PlannedStatement, set_index: u32, error: impl Into<String>) -> Self {
        Self {
            index: stmt.index,
            set_index,
            sql_range: stmt.sql_range.clone(),
            kind: stmt.kind,
            result: None,
            affected_rows: None,
            error: Some(error.into()),
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
    let planned = mystique_sql::plan_batch(text, dialect);
    if planned.is_empty() {
        return Err(ConnectorError::Policy("Query is empty.".into()));
    }

    let mut names = Vec::new();
    let mut statements = Vec::with_capacity(planned.len());
    for stmt in planned {
        names.extend(stmt.destructive_names.iter().cloned());
        statements.push(PlannedStatement {
            index: stmt.index,
            sql: stmt.sql,
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

    if let Some(stmt) = statements.iter().find(|stmt| stmt.txn_boundary) {
        return Err(ConnectorError::Query(format!(
            "Statement {} starts or ends a transaction ({}). Nothing ran.",
            display_index(stmt.index),
            sql_snippet(&stmt.sql)
        )));
    }
    if mode == QueryMode::Read {
        if let Some(stmt) = statements
            .iter()
            .find(|stmt| stmt.kind != StatementKind::Read)
        {
            return Err(ConnectorError::Policy(format!(
                "Statement {} is not a read ({}). Nothing ran.",
                display_index(stmt.index),
                sql_snippet(&stmt.sql)
            )));
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

fn display_index(index: u32) -> u64 {
    u64::from(index) + 1
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
    fn pump_keeps_index_changes_and_collapses_repeats() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let pump = ProgressPump::new({
            let seen = Arc::clone(&seen);
            move |event| {
                seen.lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .push(event);
            }
        });
        let started = |index| BatchEvent {
            index,
            total: 2,
            phase: BatchPhase::Started,
        };
        pump.emit(started(0), false);
        pump.emit(started(0), false);
        pump.emit(started(1), false);
        pump.flush();
        let got = seen.lock().unwrap_or_else(|err| err.into_inner());
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[0].index, 0);
        assert_eq!(got[1].index, 1);
    }

    #[test]
    fn comment_select_is_a_read_row_route() {
        let plan = gate_sql_batch("-- note\nSELECT 1", SqlDialect::Postgres, QueryMode::Read)
            .expect("comment");
        assert!(plan.statements[0].returns_rows);
        assert!(plan.statements[0].cursor);
    }
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
