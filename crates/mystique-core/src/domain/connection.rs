use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Saved connection profile (any engine).
///
/// **Note:** passwords are stored as plaintext in the local app DB for now
/// (no keychain). Treat the app data directory as sensitive.
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
    /// True when a non-empty password is stored.
    pub password_set: bool,
    /// Plaintext password (temporary — keychain later). Omitted when empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    pub favorite: bool,
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
    /// Plaintext password to store. Empty / omitted keeps the existing secret on update.
    #[serde(default)]
    pub password: Option<String>,
    /// Legacy alias: if `password` is unset, non-empty `secret_ref` is stored as the password.
    #[serde(default)]
    pub secret_ref: Option<String>,
    /// When true, clear the stored password on update.
    #[serde(default)]
    pub clear_secret: bool,
    /// `None` leaves the stored flag unchanged on update.
    #[serde(default)]
    pub favorite: Option<bool>,
}

/// Old UI wrote keychain-style placeholders into `secret_ref` without a real secret.
pub fn is_placeholder_secret_ref(value: &str) -> bool {
    let v = value.trim();
    v.starts_with("mystique-db/connection") || v.starts_with("mystique/conn/")
}

pub fn validate_environment(env: &str) -> Result<(), String> {
    match env {
        "local" | "staging" | "production" => Ok(()),
        _ => Err(format!(
            "environment must be local|staging|production, got '{env}'"
        )),
    }
}
