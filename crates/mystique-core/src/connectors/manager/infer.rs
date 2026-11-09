//! Field inference for document stores (the `infer_fields` tool).

use std::sync::Arc;

use mystique_connector_api::{
    clamp_infer_sample, ConnectorError, ConnectorResult, FieldInference, InferFieldsInput,
    InferFieldsRequest, FETCH_TIMEOUT_MS,
};

use super::ConnectionManager;

impl ConnectionManager {
    /// Sample one collection and count its field paths and types. A read:
    /// the plugin runs a fixed, bounded pipeline, never user stages.
    pub async fn infer_fields(&self, req: InferFieldsRequest) -> ConnectorResult<FieldInference> {
        let collection = req.collection.trim().to_string();
        if collection.is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "collection is required".into(),
            ));
        }
        let run = self.begin_run(&req.session_id, None);
        let session = self.session(&req.session_id).await?;
        self.touch_session(&req.session_id);
        let input = InferFieldsInput {
            collection,
            database: req
                .database
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            sample: clamp_infer_sample(req.sample),
            timeout_ms: FETCH_TIMEOUT_MS,
            control: Arc::clone(&run.control),
        };
        self.drive_registered(&run.control, FETCH_TIMEOUT_MS, session.infer_fields(&input))
            .await
    }

    /// Put a test double in the session map under `session_id`.
    #[cfg(test)]
    pub(crate) async fn insert_test_session(
        &self,
        session_id: &str,
        session: Arc<dyn mystique_connector_api::ConnectorSession>,
        environment: &str,
        origin: super::SessionOrigin,
    ) {
        self.sessions.write().await.insert(
            session_id.into(),
            super::LiveSession {
                product_id: session.engine_id().to_string(),
                session,
                display_name: "test".into(),
                opened_at: String::new(),
                _tunnel: None,
                environment: environment.into(),
                read_only: false,
                connection_name: "test".into(),
                origin,
                profile_id: None,
                default_database: None,
                last_used: std::sync::Mutex::new(std::time::Instant::now()),
            },
        );
    }
}
