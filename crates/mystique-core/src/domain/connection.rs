use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Saved connection profile (any engine). Secrets live in the OS keychain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    /// Plugin id: `postgres`, `mongodb`, `redis`, …
    pub engine: String,
    pub color_id: String,
    pub environment: String,
    pub read_only: bool,
    pub notes: Option<String>,
    /// Engine-specific connection parameters (JSON object).
    pub plugin_config: JsonValue,
    /// Whether a keychain `secret_ref` is present (password never returned).
    pub password_set: bool,
    pub last_connected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConnectionInput {
    pub id: Option<String>,
    pub name: String,
    pub engine: String,
    pub color_id: String,
    pub environment: String,
    pub read_only: bool,
    pub notes: Option<String>,
    pub plugin_config: JsonValue,
    pub secret_ref: Option<String>,
    #[serde(default)]
    pub clear_secret: bool,
}

pub fn validate_environment(env: &str) -> Result<(), String> {
    match env {
        "local" | "staging" | "production" => Ok(()),
        _ => Err(format!(
            "environment must be local|staging|production, got '{env}'"
        )),
    }
}
