//! A MongoDB buffer with more than one shell statement.
//!
//! The engine's policy cuts the text ([`EngineQueryPolicy::split`]). Every
//! statement is validated before anything runs, so Read mode refuses a write
//! anywhere in the buffer and a typo in the last statement does not leave the
//! first ones applied. One write-guard check covers the batch at its worst
//! risk. Then the statements run in order, as a SQL batch does: progress per
//! statement, one result each, and the first error stops the batch with
//! `stopped_at` set. Cancel is checked between statements; inside one, the
//! caller's `drive_registered` stops it.

use std::ops::Range;
use std::sync::Arc;

use mystique_connector_api::{
    emit_progress, position_at, BatchPhase, ConnectorError, ConnectorResult, ConnectorSession,
    EngineQueryPolicy, ExecuteQueryRequest, PlannedStatement, ProgressPump, QueryBatchResult,
    QueryControl, QueryMode, SqlRange, StatementKind, StatementResult, ValidatedQuery, WriteAction,
    WriteRisk,
};

use super::guard::WriteCheck;
use super::ConnectionManager;

/// Values the caller already resolved from the request.
pub(super) struct DocumentRun {
    pub mode: QueryMode,
    pub max_rows: u32,
    pub timeout_ms: u64,
    pub read_only: bool,
    pub progress: Option<ProgressPump>,
    pub tab_id: Option<String>,
    pub control: Arc<QueryControl>,
}

pub(super) fn statement_kind(risk: WriteRisk) -> StatementKind {
    match risk {
        WriteRisk::Read => StatementKind::Read,
        WriteRisk::Write => StatementKind::Write,
        WriteRisk::Destructive => StatementKind::Destructive,
    }
}

/// One statement of the batch, for the plugin. Mongo statements return a
/// result set; nothing else on the plan applies to them.
pub(super) fn document_statement(
    index: u32,
    text: &str,
    range: Range<usize>,
    kind: StatementKind,
) -> PlannedStatement {
    PlannedStatement {
        index,
        sql: text.to_string(),
        sql_range: SqlRange {
            start: u32::try_from(range.start).unwrap_or(u32::MAX),
            end: u32::try_from(range.end).unwrap_or(u32::MAX),
        },
        kind,
        returns_rows: true,
        returns_count: false,
        cursor: false,
        multi_set: false,
        txn_boundary: false,
        explain_executes: false,
        params: Vec::new(),
    }
}

impl ConnectionManager {
    pub(super) async fn execute_document_script(
        &self,
        session: Arc<dyn ConnectorSession>,
        policy: Arc<dyn EngineQueryPolicy>,
        req: ExecuteQueryRequest,
        ranges: Vec<Range<usize>>,
        run: DocumentRun,
    ) -> ConnectorResult<QueryBatchResult> {
        let text = req.text.as_str();
        let mut statements = Vec::with_capacity(ranges.len());
        for (index, range) in ranges.into_iter().enumerate() {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            let body = text.get(range.clone()).ok_or_else(|| {
                ConnectorError::Internal(format!("statement range {range:?} is outside the text"))
            })?;
            if let Err(err) = policy.validate(body, run.mode) {
                return Err(nothing_ran(text, index, range.start, body, &err));
            }
            let risk = if run.mode == QueryMode::Read {
                WriteRisk::Read
            } else {
                policy.classify(body)
            };
            statements.push(document_statement(index, body, range, statement_kind(risk)));
        }
        let worst = statements
            .iter()
            .map(|stmt| stmt.kind.write_risk())
            .max()
            .unwrap_or(WriteRisk::Read);
        let preview = statements
            .iter()
            .map(|stmt| policy.op_preview(&stmt.sql))
            .collect::<Vec<_>>()
            .join("; ");
        self.enforce_write_guard(WriteCheck {
            preview,
            statements: statements.iter().map(|stmt| stmt.sql.clone()).collect(),
            confirm_object_name: req.confirm_object_name.as_deref(),
            profile_read_only: run.read_only,
            pinned: run.tab_id.is_some(),
            ..WriteCheck::new(&req.session_id, WriteAction::Query, worst)
        })
        .await?;
        if let Some(tab) = run.tab_id.as_deref() {
            self.ensure_tab_pin(&req.session_id, tab).await?;
        }

        let mut result = self
            .drive_registered(
                &run.control,
                run.timeout_ms,
                self.run_document_statements(session.as_ref(), &req, &statements, &run),
            )
            .await;
        if let Some(progress) = &run.progress {
            progress.flush();
        }
        if let Ok(batch) = result.as_mut() {
            place_document_errors(text, batch);
        }
        result
    }

    async fn run_document_statements(
        &self,
        session: &dyn ConnectorSession,
        req: &ExecuteQueryRequest,
        statements: &[PlannedStatement],
        run: &DocumentRun,
    ) -> ConnectorResult<QueryBatchResult> {
        let total = u32::try_from(statements.len()).unwrap_or(u32::MAX);
        let mut results = Vec::with_capacity(statements.len());
        for stmt in statements {
            if run.control.is_cancelled() {
                return Err(super::run::cancelled_error());
            }
            emit_progress(&run.progress, stmt.index, total, BatchPhase::Started);
            let ran = match run.tab_id.as_deref() {
                Some(tab) => {
                    self.pinned_document_batch(
                        &req.session_id,
                        tab,
                        stmt.clone(),
                        run.mode,
                        run.max_rows,
                        run.timeout_ms,
                        req.collection.clone(),
                        req.database.clone(),
                        Arc::clone(&run.control),
                    )
                    .await
                }
                None => session
                    .execute_query(&ValidatedQuery {
                        text: stmt.sql.clone(),
                        mode: run.mode,
                        max_rows: run.max_rows,
                        timeout_ms: run.timeout_ms,
                        collection: req.collection.clone(),
                        database: req.database.clone(),
                        control: Arc::clone(&run.control),
                        params: Vec::new(),
                    })
                    .await
                    .map(|result| QueryBatchResult {
                        results: vec![StatementResult::success(stmt, 0, result, None, Vec::new())],
                        stopped_at: None,
                    }),
            };
            emit_progress(&run.progress, stmt.index, total, BatchPhase::Finished);
            match ran {
                Ok(one) => {
                    let failed = one.stopped_at.is_some()
                        || one.results.iter().any(|done| done.error.is_some());
                    results.extend(one.results);
                    if failed {
                        return Ok(QueryBatchResult {
                            results,
                            stopped_at: Some(stmt.index),
                        });
                    }
                }
                Err(err) => {
                    if run.control.is_cancelled() {
                        return Err(err);
                    }
                    results.push(StatementResult::failed(stmt, 0, &err));
                    return Ok(QueryBatchResult {
                        results,
                        stopped_at: Some(stmt.index),
                    });
                }
            }
        }
        Ok(QueryBatchResult {
            results,
            stopped_at: None,
        })
    }
}

/// A statement refused before anything ran, pointed at that statement.
fn nothing_ran(
    text: &str,
    index: u32,
    start: usize,
    body: &str,
    err: &ConnectorError,
) -> ConnectorError {
    let number = u64::from(index) + 1;
    let message = match err {
        ConnectorError::Policy(_) => format!(
            "policy: Statement {number} is not a read ({}). Nothing ran.",
            snippet(body)
        ),
        other => {
            let detail = other.user_message();
            let stop = if detail.ends_with('.') { "" } else { "." };
            format!(
                "Statement {number} ({}): {detail}{stop} Nothing ran.",
                snippet(body)
            )
        }
    };
    let local = err.local_position().map_or(0, |pos| pos.offset as usize);
    let offset = u32::try_from(start.saturating_add(local)).unwrap_or(u32::MAX);
    ConnectorError::query_at(message, position_at(text, offset), Some(index))
}

/// Every failed statement points into the submitted text: where the engine
/// said, or the start of the statement when it named no character.
fn place_document_errors(text: &str, batch: &mut QueryBatchResult) {
    for stmt in &mut batch.results {
        if stmt.error.is_none() {
            continue;
        }
        let offset = stmt
            .error_position
            .as_ref()
            .map_or(stmt.sql_range.start, |pos| pos.offset);
        stmt.error_position = Some(position_at(text, offset));
    }
}

/// The statement on one line, cut at 80 characters.
fn snippet(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= 80 {
        return flat;
    }
    let cut: String = flat.chars().take(79).collect();
    format!("{cut}…")
}
