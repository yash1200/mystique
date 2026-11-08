use std::sync::Arc;

use mystique_connector_api::{
    position_at, BatchEvent, BatchPhase, ConnectorError, ConnectorResult, ExecuteQueryRequest,
    ProgressPump, QueryBatchResult, QueryMode, QueryResult, SqlRange, StatementKind,
    StatementResult, ValidatedQuery,
};

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
    /// MongoDB is not parsed as SQL; it stays one statement.
    pub async fn execute_batch(
        &self,
        req: ExecuteQueryRequest,
        progress: Option<ProgressPump>,
    ) -> ConnectorResult<QueryBatchResult> {
        self.execute_batch_in(req, progress, None).await
    }

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
        policy.validate(&req.text, mode)?;
        let risk = if mode == QueryMode::Read {
            mystique_connector_api::WriteRisk::Read
        } else {
            policy.classify(&req.text)
        };
        let preview = mystique_connector_api::mongo_op_preview(&req.text);
        self.enforce_write_guard(
            &req.session_id,
            risk,
            &preview,
            None,
            req.confirm_object_name.as_deref(),
            read_only,
        )
        .await?;

        let kind = match risk {
            mystique_connector_api::WriteRisk::Read => StatementKind::Read,
            mystique_connector_api::WriteRisk::Write => StatementKind::Write,
            mystique_connector_api::WriteRisk::Destructive => StatementKind::Destructive,
        };
        if let Some(tab) = tab_id.as_deref() {
            self.ensure_tab_pin(&req.session_id, tab).await?;
        }
        if let Some(progress) = &progress {
            progress.emit(
                BatchEvent {
                    index: 0,
                    total: 1,
                    phase: BatchPhase::Started,
                },
                false,
            );
        }
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
                        &validated.text,
                        mode,
                        max_rows,
                        timeout_ms,
                        validated.collection.clone(),
                        validated.database.clone(),
                        Arc::clone(&control),
                    ),
                )
                .await;
            if let Some(progress) = &progress {
                progress.emit(
                    BatchEvent {
                        index: 0,
                        total: 1,
                        phase: BatchPhase::Finished,
                    },
                    true,
                );
                progress.flush();
            }
            return batch;
        }
        let result = self
            .drive_registered(&control, timeout_ms, session.execute_query(&validated))
            .await;
        if let Some(progress) = &progress {
            progress.emit(
                BatchEvent {
                    index: 0,
                    total: 1,
                    phase: BatchPhase::Finished,
                },
                true,
            );
            progress.flush();
        }
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
            }],
            stopped_at: None,
        })
    }
}

/// Last successful statement, or the error that stopped the batch.
pub(super) fn place_error_positions(text: &str, batch: &mut QueryBatchResult) {
    for stmt in &mut batch.results {
        if let Some(position) = stmt.error_position.as_mut() {
            *position = position_at(text, position.offset);
        }
    }
}

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
