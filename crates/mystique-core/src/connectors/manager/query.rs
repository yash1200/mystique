use mystique_connector_api::{
    sql_destructive_object_name, ConnectorError, ConnectorResult, ExecuteQueryRequest, QueryMode,
    QueryResult, ValidatedQuery, WriteRisk, DEFAULT_MAX_ROWS, DEFAULT_TIMEOUT_MS, HARD_MAX_ROWS,
};

use super::ConnectionManager;

impl ConnectionManager {
    /// Validate via per-engine policy SPI, then execute on the live session.
    pub async fn execute_query(&self, req: ExecuteQueryRequest) -> ConnectorResult<QueryResult> {
        let session = self.session(&req.session_id).await?;

        let engine = session.engine_id().to_string();
        let mode = if req.profile_read_only {
            QueryMode::Read
        } else {
            req.mode
        };

        let policy = self.registry.policy(&engine).ok_or_else(|| {
            ConnectorError::Internal(format!("no query policy registered for engine `{engine}`"))
        })?;
        policy.validate(&req.text, mode)?;

        let risk = if mode == QueryMode::Read {
            WriteRisk::Read
        } else {
            policy.classify(&req.text)
        };
        let object_name = sql_destructive_object_name(&req.text);
        self.enforce_write_guard(
            &req.session_id,
            risk,
            &req.text,
            object_name.as_deref(),
            req.confirm_object_name.as_deref(),
            req.profile_read_only,
        )
        .await?;

        let max_rows = req
            .max_rows
            .unwrap_or(DEFAULT_MAX_ROWS)
            .clamp(1, HARD_MAX_ROWS);
        let timeout_ms = req.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS).max(1_000);

        let validated = ValidatedQuery {
            text: req.text,
            mode,
            max_rows,
            timeout_ms,
            collection: req.collection,
            database: req.database,
        };

        session.execute_query(&validated).await
    }
}
