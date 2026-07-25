use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::error::{ConnectorError, ConnectorResult};

/// Opaque engine config from the profile `plugin_config` JSON bag + secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    /// Engine id: `postgres`, `sqlite`, `mongodb`, …
    pub engine: String,
    /// Engine-specific fields (host, port, path, uri, …).
    pub plugin_config: JsonValue,
    /// Resolved secret (password) — never log this.
    #[serde(default, skip_serializing)]
    pub password: Option<String>,
    /// Optional pool overrides (all engines understand a common subset).
    #[serde(default)]
    pub pool: PoolConfig,
}

/// Cross-engine pool knobs. Plugins map these onto native pool settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolConfig {
    /// Max connections in the pool (default engine-specific).
    #[serde(default = "default_max_size")]
    pub max_size: u32,
    /// Minimum idle connections to keep warm.
    #[serde(default = "default_min_idle")]
    pub min_idle: u32,
    /// Acquire timeout in milliseconds.
    #[serde(default = "default_acquire_timeout_ms")]
    pub acquire_timeout_ms: u64,
    /// Connection idle timeout in seconds (0 = no timeout).
    #[serde(default)]
    pub idle_timeout_secs: u64,
}

fn default_max_size() -> u32 {
    10
}
fn default_min_idle() -> u32 {
    1
}
fn default_acquire_timeout_ms() -> u64 {
    10_000
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_size: default_max_size(),
            min_idle: default_min_idle(),
            acquire_timeout_ms: default_acquire_timeout_ms(),
            idle_timeout_secs: 300,
        }
    }
}

impl ConnectionConfig {
    pub fn new(engine: impl Into<String>, plugin_config: JsonValue) -> Self {
        Self {
            engine: engine.into(),
            plugin_config,
            password: None,
            pool: PoolConfig::default(),
        }
    }

    pub fn with_password(mut self, password: Option<String>) -> Self {
        self.password = password;
        self
    }

    pub fn with_pool(mut self, pool: PoolConfig) -> Self {
        self.pool = pool;
        self
    }

    pub fn str_field(&self, key: &str) -> Option<String> {
        self.plugin_config
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn u16_field(&self, key: &str, default: u16) -> u16 {
        self.plugin_config
            .get(key)
            .and_then(|v| {
                v.as_u64()
                    .map(|n| n as u16)
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .unwrap_or(default)
    }

    pub fn bool_field(&self, key: &str, default: bool) -> bool {
        self.plugin_config
            .get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(default)
    }

    pub fn require_str(&self, key: &str) -> ConnectorResult<String> {
        self.str_field(key)
            .ok_or_else(|| ConnectorError::InvalidConfig(format!("missing `{key}`")))
    }
}
