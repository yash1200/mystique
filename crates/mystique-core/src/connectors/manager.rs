use std::collections::HashMap;

use chrono::Utc;
use mystique_connector_api::{
    ConnectionConfig, ConnectorError, ConnectorResult, ConnectorSession, EngineDescriptor,
    PoolConfig, PoolStatus, SessionInfo, TestResult,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::ConnectorRegistry;

/// API request to open a pooled session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionRequest {
    pub engine: String,
    pub plugin_config: JsonValue,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub pool: Option<PoolConfig>,
    /// Optional client-supplied id (defaults to UUID).
    #[serde(default)]
    pub session_id: Option<String>,
}

struct LiveSession {
    session: Box<dyn ConnectorSession>,
    display_name: String,
    opened_at: String,
}

/// Tracks open engine sessions (each session owns its connection pool).
pub struct ConnectionManager {
    registry: ConnectorRegistry,
    sessions: RwLock<HashMap<String, LiveSession>>,
}

impl ConnectionManager {
    pub fn new(registry: ConnectorRegistry) -> Self {
        Self {
            registry,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub fn bootstrap() -> Self {
        Self::new(ConnectorRegistry::bootstrap())
    }

    pub fn registry(&self) -> &ConnectorRegistry {
        &self.registry
    }

    pub fn list_engines(&self) -> Vec<EngineDescriptor> {
        self.registry.list()
    }

    /// Always returns a structured [`TestResult`] (never fails the API envelope for connect errors).
    /// `ok: true` means the engine accepted the credentials and answered a ping/version probe.
    pub async fn test_connection(&self, req: &OpenSessionRequest) -> ConnectorResult<TestResult> {
        let start = std::time::Instant::now();
        let connector = match self.registry.get(&req.engine) {
            Some(c) => c,
            None => {
                return Ok(TestResult {
                    ok: false,
                    engine: req.engine.clone(),
                    server_version: None,
                    latency_ms: start.elapsed().as_millis() as u64,
                    message: format!("Unknown engine `{}`", req.engine),
                });
            }
        };

        let config = ConnectionConfig {
            engine: req.engine.clone(),
            plugin_config: req.plugin_config.clone(),
            password: req.password.clone(),
            pool: req.pool.clone().unwrap_or_default(),
        };

        match connector.test_connection(&config).await {
            Ok(mut result) => {
                // Ensure latency is populated even if plugin left it at 0
                if result.latency_ms == 0 {
                    result.latency_ms = start.elapsed().as_millis() as u64;
                }
                Ok(result)
            }
            Err(e) => Ok(TestResult {
                ok: false,
                engine: req.engine.clone(),
                server_version: None,
                latency_ms: start.elapsed().as_millis() as u64,
                message: e.to_string(),
            }),
        }
    }

    pub async fn open_session(&self, req: OpenSessionRequest) -> ConnectorResult<SessionInfo> {
        let connector = self
            .registry
            .get(&req.engine)
            .ok_or_else(|| ConnectorError::InvalidConfig(format!("unknown engine `{}`", req.engine)))?;

        let config = ConnectionConfig {
            engine: req.engine.clone(),
            plugin_config: req.plugin_config.clone(),
            password: req.password.clone(),
            pool: req.pool.clone().unwrap_or_default(),
        };

        let session = connector.connect(&config).await?;
        let session_id = req
            .session_id
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let info = SessionInfo {
            session_id: session_id.clone(),
            engine: session.engine_id().to_string(),
            display_name: connector.display_name().to_string(),
            capabilities: session.capabilities(),
            pool: session.pool_status(),
            opened_at: Utc::now().to_rfc3339(),
        };

        let live = LiveSession {
            session,
            display_name: info.display_name.clone(),
            opened_at: info.opened_at.clone(),
        };

        // Replace previous session with same id if any
        let mut guard = self.sessions.write().await;
        if let Some(prev) = guard.remove(&session_id) {
            let _ = prev.session.close().await;
        }
        guard.insert(session_id, live);
        Ok(info)
    }

    pub async fn close_session(&self, session_id: &str) -> ConnectorResult<()> {
        let mut guard = self.sessions.write().await;
        if let Some(live) = guard.remove(session_id) {
            live.session.close().await?;
            Ok(())
        } else {
            Err(ConnectorError::InvalidConfig(format!(
                "session not found: {session_id}"
            )))
        }
    }

    pub async fn ping(&self, session_id: &str) -> ConnectorResult<()> {
        let guard = self.sessions.read().await;
        let live = guard
            .get(session_id)
            .ok_or_else(|| ConnectorError::InvalidConfig(format!("session not found: {session_id}")))?;
        live.session.ping().await
    }

    pub async fn pool_status(&self, session_id: &str) -> ConnectorResult<Option<PoolStatus>> {
        let guard = self.sessions.read().await;
        let live = guard
            .get(session_id)
            .ok_or_else(|| ConnectorError::InvalidConfig(format!("session not found: {session_id}")))?;
        Ok(live.session.pool_status())
    }

    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let guard = self.sessions.read().await;
        guard
            .iter()
            .map(|(id, live)| SessionInfo {
                session_id: id.clone(),
                engine: live.session.engine_id().to_string(),
                display_name: live.display_name.clone(),
                capabilities: live.session.capabilities(),
                pool: live.session.pool_status(),
                opened_at: live.opened_at.clone(),
            })
            .collect()
    }

    pub async fn get_session(&self, session_id: &str) -> Option<SessionInfo> {
        let guard = self.sessions.read().await;
        guard.get(session_id).map(|live| SessionInfo {
            session_id: session_id.to_string(),
            engine: live.session.engine_id().to_string(),
            display_name: live.display_name.clone(),
            capabilities: live.session.capabilities(),
            pool: live.session.pool_status(),
            opened_at: live.opened_at.clone(),
        })
    }
}
