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

/// TOFU / mismatch prompt for an SSH bastion host key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshHostKeyPrompt {
    /// Always `sshHostKey` so the UI can JSON.parse IPC errors.
    pub code: String,
    /// `unknown` (first connect) or `mismatch` (key changed).
    pub kind: String,
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
    pub key_type: String,
    #[serde(default)]
    pub public_key: String,
}

impl SshHostKeyPrompt {
    pub fn unknown(
        host: String,
        port: u16,
        fingerprint: String,
        key_type: String,
        public_key: String,
    ) -> Self {
        Self {
            code: "sshHostKey".into(),
            kind: "unknown".into(),
            host,
            port,
            fingerprint,
            key_type,
            public_key,
        }
    }

    pub fn mismatch(
        host: String,
        port: u16,
        fingerprint: String,
        key_type: String,
        public_key: String,
    ) -> Self {
        Self {
            code: "sshHostKey".into(),
            kind: "mismatch".into(),
            host,
            port,
            fingerprint,
            key_type,
            public_key,
        }
    }
}

impl std::fmt::Display for SshHostKeyPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match serde_json::to_string(self) {
            Ok(s) => f.write_str(&s),
            Err(_) => write!(
                f,
                "SSH host key {} for {}:{}",
                self.kind, self.host, self.port
            ),
        }
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_host_key: Option<SshHostKeyPrompt>,
}
