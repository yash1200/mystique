use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Saved connection profile (any engine).
///
/// List/get never include a password. Secrets are vault envelopes in the
/// `secrets` table; `secret_ref` is [`VAULT_SECRET_REF`] for those, else null.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub plugin_config: JsonValue,
    /// True when a password is saved in the vault.
    pub password_set: bool,
    /// False: the password is asked once per app session and kept in memory.
    #[serde(default = "default_true")]
    pub save_password: bool,
    /// SSH bastion (transport). Empty / disabled when unused.
    #[serde(default)]
    pub ssh: crate::domain::SshConfig,
    /// True when a bastion password or key passphrase is saved.
    #[serde(default)]
    pub ssh_password_set: bool,
    /// Always omitted on list/get. Write-only on save via [`SaveConnectionInput`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    pub favorite: bool,
    /// Query timeout for this profile. `None` uses the app setting; 0 is none.
    #[serde(default)]
    pub query_timeout_secs: Option<u32>,
    pub last_connected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct SaveConnectionInput {
    pub id: Option<String>,
    pub name: String,
    pub engine: String,
    pub color_id: String,
    pub environment: String,
    pub read_only: bool,
    pub notes: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub plugin_config: JsonValue,
    /// Plaintext password to store. Empty / omitted keeps the existing secret on update.
    #[serde(default)]
    pub password: Option<String>,
    /// Ignored. Only `password` is saved.
    #[serde(default)]
    pub secret_ref: Option<String>,
    /// When true, clear the stored password on update.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub clear_secret: bool,
    /// After insert, copy the saved secrets from this profile id (duplicate).
    #[serde(default)]
    pub copy_secret_from: Option<String>,
    /// `None` leaves the stored flag unchanged on update.
    #[serde(default)]
    pub favorite: Option<bool>,
    /// SSH bastion. Omitted / default = no tunnel.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub ssh_config: crate::domain::SshConfig,
    /// Bastion password or key passphrase. Empty keeps the existing secret.
    #[serde(default)]
    pub ssh_password: Option<String>,
    /// Clear the stored SSH secret on update.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub clear_ssh_secret: bool,
    /// Passphrase for an encrypted TLS client key.
    #[serde(default)]
    pub tls_key_passphrase: Option<String>,
    /// `None` uses the app setting; 0 is no timeout. Saved as sent, also on update.
    #[serde(default)]
    pub query_timeout_secs: Option<u32>,
    /// False deletes any saved DB password and asks once per session.
    /// `None` keeps the stored flag on update and saves on create.
    #[serde(default)]
    pub save_password: Option<bool>,
}

fn default_true() -> bool {
    true
}

/// `secret_ref` / `ssh_secret_ref` value: the secret is an envelope in `secrets`.
pub const VAULT_SECRET_REF: &str = "vault";

/// Keychain item name from builds before integer ids (`connection.{id}.password`).
/// Only the keychain cleanup matches it.
pub fn is_legacy_keychain_pointer(account: &str) -> bool {
    let t = account.trim();
    t.starts_with("connection.")
        && t.ends_with(".password")
        && t.len() > "connection..password".len()
}

pub fn validate_environment(env: &str) -> Result<(), String> {
    match env {
        "local" | "staging" | "production" => Ok(()),
        _ => Err(format!(
            "environment must be local|staging|production, got '{env}'"
        )),
    }
}
