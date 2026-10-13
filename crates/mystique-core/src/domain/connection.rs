use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Saved connection profile (any engine).
///
/// List/get never include a password. Secrets live in the OS keychain;
/// SQLite `secret_ref` is an opaque keychain account name.
/// New secrets use `conn.<install-uuid>.<id>`. Older rows keep `conn.<id>`.
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
    /// True when a non-empty password is stored in the OS keychain.
    pub password_set: bool,
    /// SSH bastion (transport). Empty / disabled when unused.
    #[serde(default)]
    pub ssh: crate::domain::SshConfig,
    /// True when a bastion password or key passphrase is in the keychain.
    #[serde(default)]
    pub ssh_password_set: bool,
    /// Always omitted on list/get. Write-only on save via [`SaveConnectionInput`].
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
    /// Legacy alias: if `password` is unset, non-empty `secret_ref` is treated as the password
    /// when it is not a keychain pointer or placeholder.
    #[serde(default)]
    pub secret_ref: Option<String>,
    /// When true, clear the stored password on update.
    #[serde(default)]
    pub clear_secret: bool,
    /// After insert, copy the keychain secret from this profile id (duplicate).
    #[serde(default)]
    pub copy_secret_from: Option<String>,
    /// `None` leaves the stored flag unchanged on update.
    #[serde(default)]
    pub favorite: Option<bool>,
    /// SSH bastion. Omitted / default = no tunnel.
    #[serde(default)]
    pub ssh_config: crate::domain::SshConfig,
    /// Bastion password or key passphrase. Empty keeps the existing secret.
    #[serde(default)]
    pub ssh_password: Option<String>,
    /// Clear the stored SSH secret on update.
    #[serde(default)]
    pub clear_ssh_secret: bool,
    /// Passphrase for an encrypted TLS client key.
    #[serde(default)]
    pub tls_key_passphrase: Option<String>,
}

/// Old UI wrote keychain-style placeholders into `secret_ref` without a real secret.
pub fn is_placeholder_secret_ref(value: &str) -> bool {
    let v = value.trim();
    v.starts_with("mystique-db/connection") || v.starts_with("mystique/conn/")
}

/// Legacy keychain account `conn.<id>`. Reads use the stored pointer, not this.
pub fn secret_account(connection_id: &str) -> String {
    format!("conn.{connection_id}")
}

pub fn tls_secret_account(connection_id: &str) -> String {
    format!("conn.{connection_id}.tls")
}

/// Password account scoped to one install of the app database.
///
/// `install_id` is the UUID in `app_settings.install_id`. A new database
/// gets a new id, so a recycled integer cannot open the previous install's item.
pub fn scoped_secret_account(install_id: &str, connection_id: &str) -> String {
    format!("conn.{}.{}", install_id.trim(), connection_id.trim())
}

pub fn scoped_ssh_secret_account(install_id: &str, connection_id: &str) -> String {
    format!("{}.ssh", scoped_secret_account(install_id, connection_id))
}

pub fn scoped_tls_secret_account(install_id: &str, connection_id: &str) -> String {
    format!("{}.tls", scoped_secret_account(install_id, connection_id))
}

/// Mongo/DocumentDB URI username. Sibling of the password account.
pub fn user_secret_account(password_account: &str) -> String {
    format!("{}.user", password_account.trim())
}

pub fn is_keychain_pointer(secret_ref: &str) -> bool {
    let t = secret_ref.trim();
    t.starts_with("conn.") && t.len() > 5 && !is_placeholder_secret_ref(t)
}

/// Pre-integer-id keychain account (`connection.{id}.password`, Aug 2026).
pub fn is_legacy_keychain_pointer(secret_ref: &str) -> bool {
    let t = secret_ref.trim();
    t.starts_with("connection.")
        && t.ends_with(".password")
        && t.len() > "connection..password".len()
}

/// Keychain account used before `conn.<id>`.
pub fn legacy_secret_account(connection_id: &str) -> String {
    format!("connection.{}.password", connection_id.trim())
}

pub fn validate_environment(env: &str) -> Result<(), String> {
    match env {
        "local" | "staging" | "production" => Ok(()),
        _ => Err(format!(
            "environment must be local|staging|production, got '{env}'"
        )),
    }
}
