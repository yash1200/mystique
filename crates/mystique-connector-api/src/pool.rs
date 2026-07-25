use serde::{Deserialize, Serialize};

/// Snapshot of pool health for API / status bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolStatus {
    pub engine: String,
    pub max_size: u32,
    /// Currently checked-out connections (approx).
    pub active: u32,
    /// Idle connections available in the pool.
    pub idle: u32,
    /// Total connections managed by the pool.
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub ok: bool,
    pub engine: String,
    /// Server / driver version if known.
    pub server_version: Option<String>,
    pub latency_ms: u64,
    pub message: String,
}
