use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::Utc;
use mystique_connector_api::{
    forces_terminate_off, stamp_connect_overlay, ConnectionConfig, ConnectorError, ConnectorResult,
    ConnectorSession, EngineCapabilities, EngineDescriptor, EngineProduct, FormattedConnectionUrl,
    ParsedConnectionUrl, PoolConfig, PoolStatus, SessionInfo, TestResult,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::sync::{Mutex, RwLock, Semaphore};
use uuid::Uuid;

mod catalog;
mod depth;
mod guard;
mod metrics;
mod query;
mod run;
mod table;
mod transfer;

#[cfg(test)]
mod tests;

use super::ssh::{
    open_tunnel, pin_forwarded_mongo, rewrite_plugin_config_for_tunnel, ssh_target, SshTunnel,
};
use super::ConnectorRegistry;
use crate::domain::SshTunnelRequest;

fn overlay_session_caps(
    session: &dyn ConnectorSession,
    product: Option<&EngineProduct>,
) -> EngineCapabilities {
    let mut caps = session.capabilities();
    if let Some(p) = product {
        caps.engine = p.id.to_string();
        caps.apply_support_depth(p.support_depth);
        if forces_terminate_off(p.id) {
            caps.supports_terminate_session = false;
        }
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
    /// Optional one-hop SSH tunnel. Plugins still see 127.0.0.1.
    #[serde(default)]
    pub ssh: Option<SshTunnelRequest>,
    /// Passphrase for an encrypted TLS client key. Never persisted in plugin_config.
    #[serde(default)]
    pub tls_key_passphrase: Option<String>,
    /// Profile environment (`local` | `staging` | `production`). Filled from the profile.
    #[serde(default)]
    pub environment: Option<String>,
    /// Connection display name for write-guard prompts.
    #[serde(default)]
    pub connection_name: Option<String>,
    /// Profile `read_only`. Filled from the profile at attach.
    #[serde(default)]
    pub profile_read_only: bool,
    /// When false, an omitted `ssh` stays omitted. The test form sets this
    /// from the SSH checkbox so an unticked box does not reuse the saved tunnel.
    #[serde(default = "default_use_saved_ssh")]
    pub use_saved_ssh: bool,
}

fn default_use_saved_ssh() -> bool {
    true
}

struct LiveSession {
    session: Arc<dyn ConnectorSession>,
    display_name: String,
    opened_at: String,
    /// Product id the user picked (`cockroachdb`), not the runtime crate.
    product_id: String,
    /// Held so the local forward stays up until close.
    _tunnel: Option<SshTunnel>,
    environment: String,
    read_only: bool,
    connection_name: String,
}

/// Tracks open engine sessions (each session owns its connection pool).
pub struct ConnectionManager {
    registry: ConnectorRegistry,
    sessions: RwLock<HashMap<String, LiveSession>>,
    /// Session ids unlocked for writes this process (P3.1). Not persisted.
    write_unlocks: Mutex<HashSet<String>>,
    /// `execution_id` → control. Not the profile id.
    running: Mutex<HashMap<String, std::sync::Arc<mystique_connector_api::QueryControl>>>,
    /// Caps explorer/metadata fan-out. Never closed. Queries do not take a permit.
    metadata: Semaphore,
}

impl ConnectionManager {
    pub fn new(registry: ConnectorRegistry) -> Self {
        Self {
            registry,
            sessions: RwLock::new(HashMap::new()),
            write_unlocks: Mutex::new(HashSet::new()),
            running: Mutex::new(HashMap::new()),
            metadata: run::metadata_limiter(),
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
                return Ok(test_fail(
                    &req.engine,
                    start,
                    ConnectorError::InvalidConfig(format!("Unknown engine `{}`", req.engine)),
                ));
            }
        };

        let mut tunneled = match prepare_plugin_config(req).await {
            Ok(v) => v,
            Err(e) => {
                return Ok(test_fail(&req.engine, start, e));
            }
        };
        let config = connection_config(&req.engine, connector.id(), tunneled.config, req);

        let outcome = match connector.test_connection(&config).await {
            Ok(mut result) => {
                // Ensure latency is populated even if plugin left it at 0
                if result.latency_ms == 0 {
                    result.latency_ms = start.elapsed().as_millis() as u64;
                }
                Ok(result)
            }
            Err(e) => Ok(test_fail(&req.engine, start, e)),
        };
        close_tunnel(tunneled.tunnel.take()).await;
        outcome
    }

    pub async fn open_session(&self, req: OpenSessionRequest) -> ConnectorResult<SessionInfo> {
        let connector = self.registry.get(&req.engine).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("unknown engine `{}`", req.engine))
        })?;
        let product = ConnectorRegistry::product(&req.engine);
        let product_id = product
            .map(|p| p.id.to_string())
            .unwrap_or_else(|| req.engine.trim().to_string());

        let tunneled = prepare_plugin_config(&req).await?;
        let config = connection_config(&req.engine, connector.id(), tunneled.config, &req);

        let session = match connector.connect(&config).await {
            Ok(session) => session,
            Err(err) => {
                close_tunnel(tunneled.tunnel).await;
                return Err(err);
            }
        };
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
            _tunnel: tunneled.tunnel,
            environment: req
                .environment
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("local")
                .to_ascii_lowercase(),
            read_only: req.profile_read_only,
            connection_name: req
                .connection_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(&info.display_name)
                .to_string(),
        };

        // Swap under the lock; close the previous session after releasing it.
        // Mongo `Client::shutdown` can wait forever if called while we still
        // hold the map lock (or another Client clone).
        let prev = {
            let mut guard = self.sessions.write().await;
            let prev = guard.remove(&session_id);
            guard.insert(session_id.clone(), live);
            prev
        };
        self.write_unlocks.lock().await.remove(&session_id);
        if let Some(prev) = prev {
            let _ = shutdown_live(prev).await;
        }
        Ok(info)
    }

    pub async fn close_session(&self, session_id: &str) -> ConnectorResult<()> {
        let live = {
            let mut guard = self.sessions.write().await;
            guard.remove(session_id)
        };
        if let Some(live) = live {
            self.write_unlocks.lock().await.remove(session_id);
            shutdown_live(live).await?;
            Ok(())
        } else {
            Err(ConnectorError::InvalidConfig(format!(
                "session not found: {session_id}"
            )))
        }
    }

    pub async fn ping(&self, session_id: &str) -> ConnectorResult<()> {
        let session = self.session(session_id).await?;
        session.ping().await
    }

    /// Allow writes on this live session until close or process exit (P3.1).
    pub async fn unlock_writes(&self, session_id: &str) -> ConnectorResult<()> {
        let id = session_id.trim();
        if id.is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "sessionId is required.".into(),
            ));
        }
        let _ = self.session(id).await?;
        self.write_unlocks.lock().await.insert(id.to_string());
        Ok(())
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
        let (session, _) = self.session_and_read_only(session_id).await?;
        Ok(session)
    }

    /// Session plus the product overlay. The map lock is dropped before return.
    pub(super) async fn session_meta(&self, session_id: &str) -> ConnectorResult<SessionMeta> {
        let guard = self.sessions.read().await;
        let live = guard.get(session_id).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("session not found: {session_id}"))
        })?;
        Ok(SessionMeta {
            session: std::sync::Arc::clone(&live.session),
            product_id: live.product_id.clone(),
            caps: overlay_session_caps(
                live.session.as_ref(),
                ConnectorRegistry::product(&live.product_id),
            ),
        })
    }

    /// The session plus the profile read-only bit stored at open.
    ///
    /// Callers must use this bit, not a client-sent `profile_read_only: false`,
    /// to decide whether a query is allowed to write.
    pub(super) async fn session_and_read_only(
        &self,
        session_id: &str,
    ) -> ConnectorResult<(std::sync::Arc<dyn ConnectorSession>, bool)> {
        let guard = self.sessions.read().await;
        let live = guard.get(session_id).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("session not found: {session_id}"))
        })?;
        Ok((std::sync::Arc::clone(&live.session), live.read_only))
    }
}

pub(super) struct SessionMeta {
    pub session: std::sync::Arc<dyn ConnectorSession>,
    pub product_id: String,
    pub caps: EngineCapabilities,
}

fn connection_config(
    product_id: &str,
    engine: &str,
    mut plugin_config: JsonValue,
    req: &OpenSessionRequest,
) -> ConnectionConfig {
    if let Some(pw) = req
        .tls_key_passphrase
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(obj) = plugin_config.as_object_mut() {
            obj.insert(
                "clientKeyPassphrase".into(),
                JsonValue::String(pw.to_string()),
            );
        }
    }
    stamp_connect_overlay(product_id, &mut plugin_config);
    ConnectionConfig {
        engine: engine.to_string(),
        plugin_config,
        password: req.password.clone(),
        pool: req.pool.clone().unwrap_or_default(),
    }
}

struct PreparedConfig {
    config: JsonValue,
    tunnel: Option<SshTunnel>,
}

async fn shutdown_live(mut live: LiveSession) -> ConnectorResult<()> {
    let closed = live.session.close().await;
    close_tunnel(live._tunnel.take()).await;
    closed
}

async fn close_tunnel(tunnel: Option<SshTunnel>) {
    if let Some(tunnel) = tunnel {
        tunnel.close().await;
    }
}

async fn prepare_plugin_config(req: &OpenSessionRequest) -> ConnectorResult<PreparedConfig> {
    let Some(ssh) = req.ssh.as_ref() else {
        return Ok(PreparedConfig {
            config: req.plugin_config.clone(),
            tunnel: None,
        });
    };
    let (dest_host, dest_port) = ssh_target(&req.plugin_config)?;
    let tunnel = open_tunnel(ssh, &dest_host, dest_port).await?;
    let config =
        match rewrite_plugin_config_for_tunnel(req.plugin_config.clone(), tunnel.local_port) {
            Ok(config) => pin_forwarded_mongo(&req.engine, config),
            Err(err) => {
                tunnel.close().await;
                return Err(err);
            }
        };
    Ok(PreparedConfig {
        config,
        tunnel: Some(tunnel),
    })
}

fn test_fail(engine: &str, start: std::time::Instant, err: ConnectorError) -> TestResult {
    if let ConnectorError::SshHostKey(prompt) = err {
        return TestResult {
            ok: false,
            engine: engine.to_string(),
            server_version: None,
            latency_ms: start.elapsed().as_millis() as u64,
            message: format!(
                "SSH host key {}: {} ({}:{})",
                prompt.kind, prompt.fingerprint, prompt.host, prompt.port
            ),
            ssh_host_key: Some(prompt),
        };
    }
    TestResult {
        ok: false,
        engine: engine.to_string(),
        server_version: None,
        latency_ms: start.elapsed().as_millis() as u64,
        message: err.to_string(),
        ssh_host_key: None,
    }
}
