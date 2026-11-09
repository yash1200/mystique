use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Saved connection profile (any engine).
///
/// List/get never include a password. Secrets are vault envelopes in the
/// `secrets` table; `secret_ref` is [`VAULT_SECRET_REF`] for those. Older rows
/// still name a keychain account until their first use moves it.
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
    /// True when a password is saved (vault, or an old keychain item).
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
    /// Databases (or schemas) whose environment is not `environment`.
    /// Empty: `environment` applies to every database. Set with
    /// [`crate::ProfileService::set_database_environment`], not on save.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub database_environments: Vec<mystique_connector_api::DatabaseEnvironment>,
    pub last_connected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Most marks one profile keeps.
pub const MAX_DATABASE_ENVIRONMENTS: usize = 256;

/// Add, change or (with `environment: None`) remove one database mark.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct SetDatabaseEnvironmentInput {
    pub profile_id: String,
    pub database: String,
    /// Postgres schema. Omitted marks the whole database.
    #[serde(default)]
    pub schema: Option<String>,
    /// `local` | `staging` | `production`; omitted removes the mark.
    #[serde(default)]
    pub environment: Option<String>,
}

/// `marks` with one mark set or removed. Names compare case-insensitively.
pub fn with_database_environment(
    mut marks: Vec<mystique_connector_api::DatabaseEnvironment>,
    database: &str,
    schema: Option<&str>,
    environment: Option<&str>,
) -> Result<Vec<mystique_connector_api::DatabaseEnvironment>, String> {
    let database = database.trim();
    if database.is_empty() {
        return Err("database is required".into());
    }
    let schema = schema.map(str::trim).filter(|s| !s.is_empty());
    let same = |m: &mystique_connector_api::DatabaseEnvironment| {
        m.database.trim().eq_ignore_ascii_case(database)
            && m.schema.as_deref().map(str::trim).filter(|s| !s.is_empty()) == schema
    };
    marks.retain(|m| !same(m));
    if let Some(environment) = environment {
        let environment = environment.trim();
        validate_environment(environment)?;
        if marks.len() >= MAX_DATABASE_ENVIRONMENTS {
            return Err(format!(
                "a connection keeps at most {MAX_DATABASE_ENVIRONMENTS} database marks"
            ));
        }
        marks.push(mystique_connector_api::DatabaseEnvironment {
            database: database.to_string(),
            schema: schema.map(str::to_string),
            environment: environment.to_string(),
        });
    }
    Ok(marks)
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
    /// Legacy alias: if `password` is unset, non-empty `secret_ref` is treated as the password
    /// when it is not a keychain pointer or placeholder.
    #[serde(default)]
    pub secret_ref: Option<String>,
    /// When true, clear the stored password on update.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub clear_secret: bool,
    /// After insert, copy the keychain secret from this profile id (duplicate).
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
