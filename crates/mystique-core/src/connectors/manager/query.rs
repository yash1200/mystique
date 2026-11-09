use std::sync::Arc;

use mystique_connector_api::{
    emit_progress, finish_progress, BatchPhase, ConnectorError, ConnectorResult,
    ExecuteQueryRequest, ProgressPump, QueryBatchResult, QueryMode, QueryResult, SqlRange,
    StatementResult, ValidatedQuery, WriteAction,
};

use super::document::{document_statement, statement_kind, DocumentRun};
use super::guard::WriteCheck;
use super::ConnectionManager;

impl ConnectionManager {
    /// Validate via per-engine policy SPI, then execute on the live session.
    ///
    /// One [`QueryResult`]: the last statement, or the error that stopped the batch.
    /// The query canvas uses [`ConnectionManager::execute_batch`] so earlier
    /// result sets stay available.
    pub async fn execute_query(&self, req: ExecuteQueryRequest) -> ConnectorResult<QueryResult> {
        fold_batch(self.execute_batch(req, None).await?)
    }

    /// Run a batch on one pooled connection.
    ///
    /// SQL is planned once. Read mode rejects a non-read before anything runs.
    /// `BEGIN` / `COMMIT` / `ROLLBACK` are refused unless the tab is pinned.
    /// MongoDB is not parsed as SQL; its policy splits the shell text, and a
    /// single statement runs as typed.
    pub async fn execute_batch(
        &self,
        req: ExecuteQueryRequest,
        progress: Option<ProgressPump>,
    ) -> ConnectorResult<QueryBatchResult> {
        self.execute_batch_in(req, progress, None).await
    }

    // Values the caller already resolved from the request; one call site.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_document_batch(
        &self,
        session: Arc<dyn mystique_connector_api::ConnectorSession>,
        req: ExecuteQueryRequest,
        mode: QueryMode,
        max_rows: u32,
        timeout_ms: u64,
        read_only: bool,
        progress: Option<ProgressPump>,
        tab_id: Option<String>,
        control: Arc<mystique_connector_api::QueryControl>,
    ) -> ConnectorResult<QueryBatchResult> {
        let engine = session.engine_id().to_string();
        let policy = self.registry.policy(&engine).ok_or_else(|| {
            ConnectorError::Internal(format!("no query policy registered for engine `{engine}`"))
        })?;
        let ranges = policy.split(&req.text);
        if ranges.len() > 1 {
            let run = DocumentRun {
                mode,
                max_rows,
                timeout_ms,
                read_only,
                progress,
                tab_id,
                control,
            };
            return self
                .execute_document_script(session, policy, req, ranges, run)
                .await;
        }
        // One statement (or none): the whole text, exactly as typed.
        policy.validate(&req.text, mode)?;
        let risk = if mode == QueryMode::Read {
            mystique_connector_api::WriteRisk::Read
        } else {
            policy.classify(&req.text)
        };
        self.enforce_write_guard(WriteCheck {
            preview: policy.op_preview(&req.text),
            statements: vec![req.text.clone()],
            confirm_object_name: req.confirm_object_name.as_deref(),
            profile_read_only: read_only,
            pinned: tab_id.is_some(),
            request: Some(&req.text),
            ..WriteCheck::new(&req.session_id, WriteAction::Query, risk)
        })
        .await?;

        let kind = statement_kind(risk);
        if let Some(tab) = tab_id.as_deref() {
            self.ensure_tab_pin(&req.session_id, tab).await?;
        }
        emit_progress(&progress, 0, 1, BatchPhase::Started);
        let validated = ValidatedQuery {
            text: req.text,
            mode,
            max_rows,
            timeout_ms,
            collection: req.collection,
            database: req.database,
            control: Arc::clone(&control),
            params: Vec::new(),
        };
        if let Some(tab) = tab_id.as_deref() {
            let batch = self
                .drive_registered(
                    &control,
                    timeout_ms,
                    self.pinned_document_batch(
                        &req.session_id,
                        tab,
                        document_statement(0, &validated.text, 0..validated.text.len(), kind),
                        mode,
                        max_rows,
                        timeout_ms,
                        validated.collection.clone(),
                        validated.database.clone(),
                        Arc::clone(&control),
                    ),
                )
                .await;
            finish_progress(&progress);
            return batch;
        }
        let result = self
            .drive_registered(&control, timeout_ms, session.execute_query(&validated))
            .await;
        finish_progress(&progress);
        let result = result?;
        let end = u32::try_from(validated.text.len()).unwrap_or(u32::MAX);
        Ok(QueryBatchResult {
            results: vec![StatementResult {
                index: 0,
                set_index: 0,
                sql_range: SqlRange { start: 0, end },
                kind,
                result: Some(result),
                affected_rows: None,
                error: None,
                error_position: None,
                notices: Vec::new(),
                column_origins: Vec::new(),
            }],
            stopped_at: None,
        })
    }
}

/// Last successful statement, or the error that stopped the batch.
fn fold_batch(batch: QueryBatchResult) -> ConnectorResult<QueryResult> {
    if let Some(index) = batch.stopped_at {
        let message = batch
            .results
            .iter()
            .rev()
            .find(|stmt| stmt.index == index)
            .and_then(|stmt| stmt.error.clone())
            .unwrap_or_else(|| format!("Statement {} failed.", u64::from(index) + 1));
        return Err(ConnectorError::Query(message));
    }
    batch
        .results
        .iter()
        .rev()
        .find_map(|stmt| stmt.result.clone())
        .ok_or_else(|| ConnectorError::Query("No statements to run.".into()))
}
