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
}
