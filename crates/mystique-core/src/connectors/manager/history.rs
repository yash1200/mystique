//! Query history written from the connection manager.
//!
//! UI, agent, and MCP all run through `execute_batch_in`. The canvas does
//! not insert rows. A failed write does not change the query result.
//! SQL text is stored. URI userinfo and a database value that is a URI are not.
//! Statement parameters are not a column.

use std::sync::Arc;
use std::time::Instant;

use mystique_connector_api::{
    ConnectorResult, QueryBatchResult, QueryResult, SqlRange, StatementResult,
};

use crate::domain::{redact_history_text, AppendHistoryInput, QueryHistorySource};

use super::{ConnectionManager, SessionOrigin};

pub(crate) trait QueryHistoryWriter: Send + Sync {
    fn append_queries(&self, rows: Vec<AppendHistoryInput>);
}

pub(super) struct HistoryContext {
    pub connection_id: Option<String>,
    pub engine: String,
    pub source: QueryHistorySource,
}

impl ConnectionManager {
    pub(crate) fn set_query_history_writer(&self, writer: Arc<dyn QueryHistoryWriter>) {
        if let Ok(mut slot) = self.history.lock() {
            *slot = Some(writer);
        }
    }

    fn history_writer(&self) -> Option<Arc<dyn QueryHistoryWriter>> {
        self.history.lock().ok().and_then(|slot| slot.clone())
    }

    pub(super) async fn history_context(&self, session_id: &str) -> Option<HistoryContext> {
        let guard = self.sessions.read().await;
        let live = guard.get(session_id)?;
        Some(HistoryContext {
            connection_id: live.profile_id.clone(),
            engine: live.product_id.clone(),
            source: match live.origin {
                SessionOrigin::Ui => QueryHistorySource::Ui,
                SessionOrigin::Agent => QueryHistorySource::Agent,
                SessionOrigin::Mcp => QueryHistorySource::Mcp,
            },
        })
    }

    pub(super) async fn persist_history(&self, rows: Vec<AppendHistoryInput>) {
        if rows.is_empty() {
            return;
        }
        let Some(writer) = self.history_writer() else {
            return;
        };
        let task = tokio::task::spawn_blocking(move || writer.append_queries(rows));
        let _ = task.await;
    }
}

pub(super) fn rows_from_outcome(
    ctx: &HistoryContext,
    text: &str,
    database: Option<&str>,
    elapsed_ms: i64,
    outcome: &ConnectorResult<QueryBatchResult>,
) -> Vec<AppendHistoryInput> {
    let database = stored_database(database);
    match outcome {
        Ok(batch) => rows_from_results(ctx, text, database, elapsed_ms, &batch.results),
        Err(err) => {
            let body = text.trim();
            if body.is_empty() {
                return Vec::new();
            }
            vec![to_input(
                ctx,
                Draft {
                    index: 0,
                    sql: body.to_string(),
                    success: false,
                    row_count: None,
                    affected_rows: None,
                    duration_ms: Some(elapsed_ms),
                    error_message: Some(err.user_message()),
                },
                database,
            )]
        }
    }
}

fn rows_from_results(
    ctx: &HistoryContext,
    text: &str,
    database: Option<String>,
    elapsed_ms: i64,
    results: &[StatementResult],
) -> Vec<AppendHistoryInput> {
    let mut drafts: Vec<Draft> = Vec::new();
    for stmt in results {
        let sql = statement_sql(text, &stmt.sql_range);
        if let Some(prev) = drafts.last_mut() {
            if prev.index == stmt.index {
                merge(prev, stmt, elapsed_ms);
                continue;
            }
        }
        drafts.push(Draft::from_statement(stmt.index, sql, stmt, elapsed_ms));
    }
    drafts
        .into_iter()
        .filter(|draft| !draft.sql.is_empty())
        .map(|draft| to_input(ctx, draft, database.clone()))
        .collect()
}

struct Draft {
    index: u32,
    sql: String,
    success: bool,
    row_count: Option<i64>,
    affected_rows: Option<i64>,
    duration_ms: Option<i64>,
    error_message: Option<String>,
}

impl Draft {
    fn from_statement(index: u32, sql: String, stmt: &StatementResult, elapsed_ms: i64) -> Self {
        let mut draft = Self {
            index,
            sql,
            success: stmt.error.is_none(),
            row_count: None,
            affected_rows: as_i64_opt(stmt.affected_rows),
            duration_ms: Some(elapsed_ms),
            error_message: stmt.error.clone(),
        };
        apply_result(&mut draft, stmt.result.as_ref());
        draft
    }
}

fn merge(draft: &mut Draft, stmt: &StatementResult, elapsed_ms: i64) {
    if let Some(err) = stmt.error.clone() {
        draft.success = false;
        if draft.error_message.is_none() {
            draft.error_message = Some(err);
        }
    }
    if let Some(rows) = as_i64_opt(stmt.affected_rows) {
        draft.affected_rows = Some(rows);
    }
    if draft.duration_ms.is_none() {
        draft.duration_ms = Some(elapsed_ms);
    }
    apply_result(draft, stmt.result.as_ref());
}

fn apply_result(draft: &mut Draft, result: Option<&QueryResult>) {
    let Some(result) = result else {
        return;
    };
    draft.row_count = Some(as_i64(result.row_count));
    draft.duration_ms = Some(as_i64(result.duration_ms));
}

fn to_input(ctx: &HistoryContext, draft: Draft, database: Option<String>) -> AppendHistoryInput {
    let engine = ctx.engine.trim();
    AppendHistoryInput {
        connection_id: ctx.connection_id.clone(),
        engine: if engine.is_empty() {
            None
        } else {
            Some(engine.to_string())
        },
        body: redact_history_text(&draft.sql),
        language: Some(language_for(engine).to_string()),
        success: draft.success,
        row_count: draft.row_count,
        duration_ms: draft.duration_ms,
        error_message: draft.error_message.map(|msg| redact_history_text(&msg)),
        database,
        affected_rows: draft.affected_rows,
        source: ctx.source,
    }
}

fn stored_database(value: Option<&str>) -> Option<String> {
    let name = value?.trim();
    if name.is_empty() || name.len() > 256 || name.contains("://") || name.contains('@') {
        return None;
    }
    Some(name.to_string())
}

fn statement_sql(text: &str, range: &SqlRange) -> String {
    let start = usize::try_from(range.start).unwrap_or(usize::MAX);
    let end = usize::try_from(range.end).unwrap_or(usize::MAX);
    if start > end
        || end > text.len()
        || !text.is_char_boundary(start)
        || !text.is_char_boundary(end)
    {
        return text.trim().to_string();
    }
    let slice = text[start..end].trim();
    if slice.is_empty() {
        text.trim().to_string()
    } else {
        slice.to_string()
    }
}

fn language_for(engine: &str) -> &'static str {
    if engine.eq_ignore_ascii_case("mongodb") || engine.eq_ignore_ascii_case("documentdb") {
        "mql"
    } else {
        "sql"
    }
}

fn as_i64(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn as_i64_opt(n: Option<u64>) -> Option<i64> {
    n.map(as_i64)
}

pub(super) fn elapsed_ms(started: Instant) -> i64 {
    i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::{
        ConnectorError, QueryMode, SqlRange, StatementKind, StatementResult,
    };

    fn ctx(source: QueryHistorySource) -> HistoryContext {
        HistoryContext {
            connection_id: Some("7".into()),
            engine: "sqlite".into(),
            source,
        }
    }

    fn result(duration_ms: u64, row_count: u64) -> QueryResult {
        QueryResult {
            engine: "sqlite".into(),
            mode: QueryMode::Write,
            columns: Vec::new(),
            rows: Vec::new(),
            row_count,
            truncated: false,
            duration_ms,
            message: None,
        }
    }

    #[test]
    fn one_row_per_statement_keeps_affected_rows_and_drops_a_uri_database() {
        let text = "SELECT 1; INSERT INTO t (id) VALUES (1)";
        let insert_sql = "INSERT INTO t (id) VALUES (1)";
        let insert_start = text.find(insert_sql).expect("insert") as u32;
        let batch = QueryBatchResult {
            results: vec![
                StatementResult {
                    index: 0,
                    set_index: 0,
                    sql_range: SqlRange {
                        start: 0,
                        end: "SELECT 1".len() as u32,
                    },
                    kind: StatementKind::Read,
                    result: Some(result(3, 1)),
                    affected_rows: None,
                    error: None,
                    error_position: None,
                    notices: Vec::new(),
                },
                StatementResult {
                    index: 1,
                    set_index: 0,
                    sql_range: SqlRange {
                        start: insert_start,
                        end: insert_start + insert_sql.len() as u32,
                    },
                    kind: StatementKind::Write,
                    result: Some(result(5, 0)),
                    affected_rows: Some(1),
                    error: None,
                    error_position: None,
                    notices: Vec::new(),
                },
            ],
            stopped_at: None,
        };
        let rows = rows_from_outcome(
            &ctx(QueryHistorySource::Agent),
            text,
            Some("mongodb://alice:secret-token@h/db"),
            9,
            &Ok(batch),
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].body, "SELECT 1");
        assert_eq!(rows[0].source, QueryHistorySource::Agent);
        assert_eq!(rows[0].duration_ms, Some(3));
        assert!(rows[0].database.is_none());
        assert_eq!(rows[1].body, insert_sql);
        assert_eq!(rows[1].affected_rows, Some(1));
        assert_eq!(rows[1].duration_ms, Some(5));
        assert!(rows
            .iter()
            .all(|row| row.connection_id.as_deref() == Some("7")));
        assert!(rows.iter().all(|row| !row.body.contains("secret-token")));
    }

    #[test]
    fn a_rejected_batch_is_one_failed_row() {
        let rows = rows_from_outcome(
            &ctx(QueryHistorySource::Ui),
            "DELETE FROM t",
            Some("app"),
            12,
            &Err(ConnectorError::Query(
                "Statement 1 is not a read. Nothing ran.".into(),
            )),
        );
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].success);
        assert_eq!(rows[0].database.as_deref(), Some("app"));
        assert_eq!(rows[0].duration_ms, Some(12));
        assert_eq!(rows[0].body, "DELETE FROM t");
        assert!(rows[0].error_message.is_some());
    }
}
