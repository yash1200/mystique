use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use mystique_connector_api::{
    ConnectionConfig, ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities,
    EngineDescriptor, EngineProduct, FormattedConnectionUrl, ParsedConnectionUrl, PoolConfig,
    PoolStatus, SessionInfo, TestResult,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::sync::RwLock;
use uuid::Uuid;

mod catalog;
mod metrics;
mod query;
mod table;
mod transfer;

#[cfg(test)]
mod tests;

use super::ConnectorRegistry;

fn overlay_session_caps(
    session: &dyn ConnectorSession,
    product: Option<&EngineProduct>,
) -> EngineCapabilities {
    let mut caps = session.capabilities();
    if let Some(p) = product {
        caps.engine = p.id.to_string();
        caps.apply_support_depth(p.support_depth);
    }
    caps
}

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
    /// When `password` is empty, resolve the OS-keychain secret for this profile.
    #[serde(default)]
    pub profile_id: Option<String>,
}

struct LiveSession {
    session: Arc<dyn ConnectorSession>,
    display_name: String,
    opened_at: String,
    /// Product id the user picked (`cockroachdb`), not the runtime crate.
    product_id: String,
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

    /// Resolve a product id (`cockroachdb`, `mariadb`) to the runtime connector.
    fn resolve_url_connector(
        &self,
        engine: &str,
    ) -> ConnectorResult<std::sync::Arc<dyn mystique_connector_api::DatabaseConnector>> {
        if engine.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig("engine is required".into()));
        }
        self.registry
            .get(engine)
            .ok_or_else(|| ConnectorError::InvalidConfig(format!("unknown engine `{engine}`")))
    }

    /// Parse a connection URL via the engine plugin’s driver (host/port/db/user/ssl).
    ///
    /// `mariadb` is resolved to the MySQL plugin (same wire protocol / URL shape).
    pub fn parse_connection_url(
        &self,
        engine: &str,
        url: &str,
    ) -> ConnectorResult<ParsedConnectionUrl> {
        let connector = self.resolve_url_connector(engine)?;
        connector.parse_connection_url(url)
    }

    /// Format discrete plugin config into a connection URL via the engine plugin.
    pub fn format_connection_url(
        &self,
        engine: &str,
        plugin_config: JsonValue,
        password: Option<String>,
    ) -> ConnectorResult<FormattedConnectionUrl> {
        let connector = self.resolve_url_connector(engine)?;
        let config = ConnectionConfig {
            engine: connector.id().to_string(),
            plugin_config,
            password,
            pool: PoolConfig::default(),
        };
        connector.format_connection_url(&config)
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
            engine: connector.id().to_string(),
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
        let connector = self.registry.get(&req.engine).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("unknown engine `{}`", req.engine))
        })?;
        let product = ConnectorRegistry::product(&req.engine);
        let product_id = product
            .map(|p| p.id.to_string())
            .unwrap_or_else(|| req.engine.trim().to_string());

        let config = ConnectionConfig {
            engine: connector.id().to_string(),
            plugin_config: req.plugin_config.clone(),
            password: req.password.clone(),
            pool: req.pool.clone().unwrap_or_default(),
        };

        let session = connector.connect(&config).await?;
        let session: Arc<dyn ConnectorSession> = Arc::from(session);
        let session_id = req
            .session_id
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let info = SessionInfo {
            session_id: session_id.clone(),
            engine: product_id,
            display_name: product
                .map(|p| p.display_name.to_string())
                .unwrap_or_else(|| connector.display_name().to_string()),
            capabilities: overlay_session_caps(session.as_ref(), product),
            pool: session.pool_status(),
            opened_at: Utc::now().to_rfc3339(),
        };

        let live = LiveSession {
            session,
            display_name: info.display_name.clone(),
            opened_at: info.opened_at.clone(),
            product_id: info.engine.clone(),
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
        let live = guard.get(session_id).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("session not found: {session_id}"))
        })?;
        live.session.ping().await
    }

    pub async fn pool_status(&self, session_id: &str) -> ConnectorResult<Option<PoolStatus>> {
        let guard = self.sessions.read().await;
        let live = guard.get(session_id).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("session not found: {session_id}"))
        })?;
        Ok(live.session.pool_status())
    }

    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let guard = self.sessions.read().await;
        guard
            .iter()
            .map(|(id, live)| SessionInfo {
                session_id: id.clone(),
                engine: live.product_id.clone(),
                display_name: live.display_name.clone(),
                capabilities: overlay_session_caps(
                    live.session.as_ref(),
                    ConnectorRegistry::product(&live.product_id),
                ),
                pool: live.session.pool_status(),
                opened_at: live.opened_at.clone(),
            })
            .collect()
    }

    pub async fn get_session(&self, session_id: &str) -> Option<SessionInfo> {
        let guard = self.sessions.read().await;
        guard.get(session_id).map(|live| SessionInfo {
            session_id: session_id.to_string(),
            engine: live.product_id.clone(),
            display_name: live.display_name.clone(),
            capabilities: overlay_session_caps(
                live.session.as_ref(),
                ConnectorRegistry::product(&live.product_id),
            ),
            pool: live.session.pool_status(),
            opened_at: live.opened_at.clone(),
        })
    }

    pub(super) async fn session(
        &self,
        session_id: &str,
    ) -> ConnectorResult<std::sync::Arc<dyn ConnectorSession>> {
        let guard = self.sessions.read().await;
        let live = guard.get(session_id).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("session not found: {session_id}"))
        })?;
        Ok(std::sync::Arc::clone(&live.session))
    }
}
