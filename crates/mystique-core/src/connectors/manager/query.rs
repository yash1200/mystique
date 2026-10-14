use std::sync::Arc;

use mystique_connector_api::{
    clamp_timeout_ms, gate_sql_batch, BatchEvent, BatchPhase, ConnectorError, ConnectorResult,
    ExecuteQueryRequest, ProgressPump, QueryBatchResult, QueryMode, QueryResult, SqlDialect,
    SqlRange, StatementKind, StatementResult, ValidatedBatch, ValidatedQuery, DEFAULT_MAX_ROWS,
    HARD_MAX_ROWS,
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

    /// Run a batch on one connection.
    ///
    /// SQL is planned once. Read mode rejects a non-read before anything runs.
    /// `BEGIN` / `COMMIT` / `ROLLBACK` are refused until pinned sessions exist.
    /// MongoDB is not parsed as SQL; it stays one statement.
    pub async fn execute_batch(
        &self,
        req: ExecuteQueryRequest,
        progress: Option<ProgressPump>,
    ) -> ConnectorResult<QueryBatchResult> {
        let (session, live_read_only) = self.session_and_read_only(&req.session_id).await?;
        let engine = session.engine_id().to_string();
        // The live session wins. A client can only make the call stricter.
        let read_only = live_read_only || req.profile_read_only;
        let mode = if read_only { QueryMode::Read } else { req.mode };
        let max_rows = req
            .max_rows
            .unwrap_or(DEFAULT_MAX_ROWS)
            .clamp(1, HARD_MAX_ROWS);
        let timeout_ms = clamp_timeout_ms(req.timeout_ms);

        if engine.eq_ignore_ascii_case("mongodb") {
            return self
                .execute_document_batch(
                    session, req, mode, max_rows, timeout_ms, read_only, progress,
                )
                .await;
        }

        let plan = gate_sql_batch(&req.text, SqlDialect::from_engine(&engine), mode)?;
        self.enforce_write_guard(
            &req.session_id,
            plan.worst.write_risk(),
            &req.text,
            plan.destructive_name.as_deref(),
            req.confirm_object_name.as_deref(),
            read_only,
        )
        .await?;

        let (id, control) = self.begin_run(req.execution_id).await;
        let batch = ValidatedBatch {
            statements: plan.statements,
            mode,
            max_rows,
            timeout_ms,
            explain_executes: plan.explain_executes,
            collection: req.collection,
            database: req.database,
            control: Arc::clone(&control),
            progress,
        };
        let result = self
            .drive_registered(&control, timeout_ms, session.execute_batch(&batch))
            .await;
        self.finish_run(&id).await;
        result
    }

    async fn execute_document_batch(
        &self,
        session: Arc<dyn mystique_connector_api::ConnectorSession>,
        req: ExecuteQueryRequest,
        mode: QueryMode,
        max_rows: u32,
        timeout_ms: u64,
        read_only: bool,
        progress: Option<ProgressPump>,
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
        self.enforce_write_guard(
            &req.session_id,
            risk,
            &req.text,
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
        let (id, control) = self.begin_run(req.execution_id).await;
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
        };
        let result = self
            .drive_registered(&control, timeout_ms, session.execute_query(&validated))
            .await;
        self.finish_run(&id).await;
        let result = result?;
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
                notices: Vec::new(),
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
