use mystique_connector_api::{
    ConnectorError, ConnectorResult, FetchServerMetricsRequest, ServerMetricsSnapshot,
    TerminateSessionRequest, TerminateSessionResult, WriteRisk,
};

use super::ConnectionManager;

impl ConnectionManager {
    pub async fn fetch_server_metrics(
        &self,
        req: FetchServerMetricsRequest,
    ) -> ConnectorResult<ServerMetricsSnapshot> {
        if req.session_id.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "sessionId is required.".into(),
            ));
        }
        let session = self.session(&req.session_id).await?;
        session.fetch_server_metrics(&req.into_input()).await
    }

    pub async fn terminate_backend_session(
        &self,
        req: TerminateSessionRequest,
    ) -> ConnectorResult<TerminateSessionResult> {
        self.enforce_write_guard(
            &req.session_id,
            WriteRisk::Write,
            &format!("Terminate backend session {}", req.backend_session_id),
            None,
            None,
            req.profile_read_only,
        )
        .await?;
        if req.session_id.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "sessionId is required.".into(),
            ));
        }
        if req.backend_session_id.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "backendSessionId is required.".into(),
            ));
        }
        let session = self.session(&req.session_id).await?;
        session.terminate_backend_session(&req.into_input()).await
    }
}
