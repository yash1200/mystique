use std::sync::Arc;

use mystique_connector_api::{
    clamp_timeout_ms, sql_destructive_object_name_for, ConnectorError, ConnectorResult,
    ExecuteQueryRequest, QueryMode, QueryResult, SqlDialect, ValidatedQuery, WriteRisk,
    DEFAULT_MAX_ROWS, HARD_MAX_ROWS,
};

use super::ConnectionManager;

impl ConnectionManager {
    /// Validate via per-engine policy SPI, then execute on the live session.
    pub async fn execute_query(&self, req: ExecuteQueryRequest) -> ConnectorResult<QueryResult> {
        let (session, live_read_only) = self.session_and_read_only(&req.session_id).await?;

        let engine = session.engine_id().to_string();
        // The live session wins. A client can only make the call stricter.
        let read_only = live_read_only || req.profile_read_only;
        let mode = if read_only { QueryMode::Read } else { req.mode };

        let policy = self.registry.policy(&engine).ok_or_else(|| {
            ConnectorError::Internal(format!("no query policy registered for engine `{engine}`"))
        })?;
        policy.validate(&req.text, mode)?;

        let risk = if mode == QueryMode::Read {
            WriteRisk::Read
        } else {
            policy.classify(&req.text)
        };
        let object_name =
            sql_destructive_object_name_for(&req.text, SqlDialect::from_engine(&engine));
        self.enforce_write_guard(
            &req.session_id,
            risk,
            &req.text,
            object_name.as_deref(),
            req.confirm_object_name.as_deref(),
            read_only,
        )
        .await?;

        let max_rows = req
            .max_rows
            .unwrap_or(DEFAULT_MAX_ROWS)
            .clamp(1, HARD_MAX_ROWS);
        let timeout_ms = clamp_timeout_ms(req.timeout_ms);

        let (id, control) = self.begin_run(req.execution_id).await;
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
        result
    }
}
