use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::Utc;
use mystique_connector_api::{
    forces_terminate_off, stamp_connect_overlay, ConnectionConfig, ConnectorError, ConnectorResult,
    ConnectorSession, EngineCapabilities, EngineDescriptor, EngineProduct, FormattedConnectionUrl,
    ParsedConnectionUrl, PoolConfig, SessionInfo, TestResult,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::sync::{Mutex, RwLock, Semaphore};
use uuid::Uuid;

mod catalog;
mod depth;
mod explain;
mod grid_save;
mod guard;
mod history;
mod large;
mod meta_cache;
mod metrics;
mod pin;
mod query;
mod run;
mod table;
mod transfer;

pub(crate) use grid_save::GridSaveStore;
pub(crate) use history::QueryHistoryWriter;

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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionRequest {
    pub engine: String,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
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
    /// Profile `read_only`, filled from the stored profile at attach. Never
    /// read from the client: every write path checks the live session's bit.
    #[serde(skip)]
    pub profile_read_only: bool,
    /// When false, an omitted `ssh` stays omitted. The test form sets this
    /// from the SSH checkbox so an unticked box does not reuse the saved tunnel.
    #[serde(default = "default_use_saved_ssh")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub use_saved_ssh: bool,
}

fn default_use_saved_ssh() -> bool {
    true
}

/// Who opened the pool. Pins are keyed by tab id, not by this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SessionOrigin {
    #[default]
    Ui,
    Agent,
    Mcp,
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
    origin: SessionOrigin,
    /// Saved profile id. History is keyed by this, not the live session id.
    profile_id: Option<String>,
    last_used: std::sync::Mutex<std::time::Instant>,
}

/// Tracks open engine sessions (each session owns its connection pool).
pub struct ConnectionManager {
    registry: ConnectorRegistry,
    sessions: RwLock<HashMap<String, LiveSession>>,
    /// Session ids unlocked for writes this process (P3.1). Not persisted.
    write_unlocks: Mutex<HashSet<String>>,
    /// `execution_id` → control. Not the profile id.
    /// A std mutex so a run's drop guard can unregister without awaiting.
    running: std::sync::Mutex<HashMap<String, run::Inflight>>,
    /// Caps explorer/metadata fan-out. Never closed. Queries do not take a permit.
    metadata: Semaphore,
    /// Query-tab id → pinned connection.
    pins: Mutex<HashMap<String, std::sync::Arc<pin::TabPin>>>,
    /// Tabs whose open transaction was rolled back without the user asking.
    rolled_back: Mutex<HashSet<String>>,
    /// Per-id lock so `get_or_open` cannot build two pools.
    open_gates: Mutex<HashMap<String, std::sync::Arc<Mutex<()>>>>,
    /// App SQLite history. Empty in unit tests that never open the app database.
    history: std::sync::Mutex<Option<std::sync::Arc<dyn history::QueryHistoryWriter>>>,
    /// App SQLite rollback store. Empty when the app database is not open.
    grid_saves: std::sync::Mutex<Option<std::sync::Arc<dyn grid_save::GridSaveStore>>>,
    /// Catalog metadata per session and database. See `meta_cache`.
    meta: meta_cache::MetaCache,
    /// Large values read for the cell modal, paged from one read.
    large_values: large::LargeValueCache,
    /// Background exports and imports, for the queue and the quit guard.
    transfers: transfer::TransferRegistry,
}

impl ConnectionManager {
    pub fn new(registry: ConnectorRegistry) -> Self {
        Self {
            registry,
            sessions: RwLock::new(HashMap::new()),
            write_unlocks: Mutex::new(HashSet::new()),
            running: std::sync::Mutex::new(HashMap::new()),
            metadata: run::metadata_limiter(),
            pins: Mutex::new(HashMap::new()),
            rolled_back: Mutex::new(HashSet::new()),
            open_gates: Mutex::new(HashMap::new()),
            history: std::sync::Mutex::new(None),
            grid_saves: std::sync::Mutex::new(None),
            meta: meta_cache::MetaCache::default(),
            large_values: large::LargeValueCache::default(),
            transfers: transfer::TransferRegistry::default(),
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
        if let Ok(result) = &outcome {
            tracing::info!(
                engine = %req.engine,
                ok = result.ok,
                latency_ms = result.latency_ms,
                "connection test"
            );
        }
        outcome
    }

    pub async fn open_session(&self, req: OpenSessionRequest) -> ConnectorResult<SessionInfo> {
        self.open_session_as(req, SessionOrigin::Ui).await
    }

    pub(super) async fn open_session_as(
        &self,
        req: OpenSessionRequest,
        origin: SessionOrigin,
    ) -> ConnectorResult<SessionInfo> {
        let connector = self.registry.get(&req.engine).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("unknown engine `{}`", req.engine))
        })?;
        let product = ConnectorRegistry::product(&req.engine);
        let product_id = product
            .map(|p| p.id.to_string())
            .unwrap_or_else(|| req.engine.trim().to_string());

        let started = std::time::Instant::now();
        let tunneled = prepare_plugin_config(&req).await.inspect_err(|err| {
            tracing::warn!(engine = %req.engine, error = err.kind(), "session open failed: tunnel");
        })?;
        let ssh = tunneled.tunnel.is_some();
        let config = connection_config(&req.engine, connector.id(), tunneled.config, &req);

        let session = match connector.connect(&config).await {
            Ok(session) => session,
            Err(err) => {
                close_tunnel(tunneled.tunnel).await;
                tracing::warn!(
                    engine = %req.engine,
                    ssh,
                    error = err.kind(),
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "session open failed"
                );
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
            origin,
            profile_id: req
                .profile_id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string),
            last_used: std::sync::Mutex::new(std::time::Instant::now()),
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
        tracing::info!(
            session_id = %info.session_id,
            engine = %info.engine,
            origin = ?origin,
            ssh,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "session opened"
        );
        Ok(info)
    }

    pub async fn close_session(&self, session_id: &str) -> ConnectorResult<()> {
        self.rollback_pins_for_session(session_id).await;
        self.meta.invalidate(session_id);
        self.large_values.forget_session(session_id);
        let busy = self.cancel_runs_for(session_id).await;
        let live = {
            let mut guard = self.sessions.write().await;
            guard.remove(session_id)
        };
        if let Some(live) = live {
            self.write_unlocks.lock().await.remove(session_id);
            tracing::info!(session_id, engine = %live.product_id, busy, "session closed");
            if busy {
                tokio::spawn(async move {
                    let _ = shutdown_live(live).await;
                });
                Ok(())
            } else {
                shutdown_live(live).await?;
                Ok(())
            }
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

    /// A saved profile's read-only bit reaches its open sessions (desktop,
    /// agent, MCP) now, not at the next connect. Requests carry no read-only
    /// flag, so this is how an edit made while connected takes effect.
    /// Returns how many sessions were updated.
    pub async fn apply_profile_read_only(&self, profile_id: &str, read_only: bool) -> usize {
        let id = profile_id.trim();
        if id.is_empty() {
            return 0;
        }
        let mut guard = self.sessions.write().await;
        let mut updated = 0;
        for (session_id, live) in guard.iter_mut() {
            // The desktop keys its session by profile id; agent and MCP
            // sessions carry the profile id.
            if session_id == id || live.profile_id.as_deref() == Some(id) {
                live.read_only = read_only;
                updated += 1;
            }
        }
        updated
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
    /// Requests carry no read-only flag. Callers use this bit (filled from the
    /// stored profile at open, kept current by `apply_profile_read_only`)
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
            ssh_host_key: Some(*prompt),
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

#[cfg(test)]
mod tests;
#[cfg(test)]
mod transfer_tests;
