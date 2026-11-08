use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::domain::{
    is_keychain_pointer, is_legacy_keychain_pointer, is_placeholder_secret_ref,
    scoped_secret_account, scoped_ssh_secret_account, scoped_tls_secret_account, secret_account,
    ssh_secret_account, tls_secret_account, user_secret_account, ConnectionProfile,
    SaveConnectionInput,
};
use crate::error::{AppError, AppResult};
use crate::secrets::{is_legacy_plaintext_secret, SecretStore};
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

const INSTALL_ID_KEY: &str = "install_id";
/// Set once the startup cleanup of plaintext rows has run.
const SECRETS_MIGRATED_KEY: &str = "secrets.startup_migration_v1";
/// Non-secret marker in `plugin_config`: a TLS key passphrase is in the keychain.
const TLS_SAVED_KEY: &str = "clientKeyPassphraseSaved";

pub struct ProfileService<'a> {
    pub(crate) db: &'a AppDatabase,
    pub(crate) secrets: &'a SecretStore,
}

fn incoming_password(input: &SaveConnectionInput) -> Option<String> {
    if let Some(pw) = input
        .password
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(pw.to_string());
    }
    input
        .secret_ref
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !is_placeholder_secret_ref(s) && !is_keychain_pointer(s))
        .map(|s| s.to_string())
}

fn strip_plugin_password(mut input: SaveConnectionInput) -> SaveConnectionInput {
    if let Some(obj) = input.plugin_config.as_object_mut() {
        obj.remove("password");
        obj.remove("clientKeyPassphrase");
        obj.remove("client_key_passphrase");
    }
    input
}

fn config_password(config: &JsonValue) -> Option<String> {
    config
        .get("password")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Pull the Mongo password out of `plugin_config.uri` before it is written.
///
/// A form that already stripped the URI leaves `username` / `password`
/// fields beside it. The username is not a secret: it stays in the config
/// as `username`, next to the stripped URI.
fn take_mongo_credentials(input: &mut SaveConnectionInput) -> Option<String> {
    if !is_mongo(&input.engine) {
        return None;
    }
    let uri = config_text(&input.plugin_config, "uri")?;
    let field_user = config_text(&input.plugin_config, "username")
        .or_else(|| config_text(&input.plugin_config, "user"));
    let field_password = config_password(&input.plugin_config);
    let (username, password) =
        if let Some(creds) = crate::domain::mongo_uri::strip_mongo_userinfo(&uri) {
            if let Some(obj) = input.plugin_config.as_object_mut() {
                obj.insert("uri".into(), JsonValue::String(creds.uri));
            }
            let username = if creds.username.is_empty() {
                field_user
            } else {
                Some(creds.username)
            };
            let password = if field_password.is_some() {
                field_password
            } else if creds.password.is_empty() {
                None
            } else {
                Some(creds.password)
            };
            (username, password)
        } else if field_password.is_some() {
            (field_user, field_password)
        } else {
            return None;
        };
    if let Some(obj) = input.plugin_config.as_object_mut() {
        obj.remove("user");
        obj.remove("password");
        match username {
            Some(user) => obj.insert("username".into(), JsonValue::String(user)),
            None => obj.remove("username"),
        };
    }
    password
}

fn is_mongo(engine: &str) -> bool {
    mystique_connector_api::runtime_id(engine) == Some("mongodb")
}

fn config_username(config: &JsonValue) -> Option<String> {
    config_text(config, "username").or_else(|| config_text(config, "user"))
}

fn has_client_key(config: &JsonValue) -> bool {
    config_text(config, "clientKeyPath")
        .or_else(|| config_text(config, "client_key_path"))
        .is_some()
}

/// Rows saved before the marker existed have no value: those still look.
fn tls_passphrase_saved(config: &JsonValue) -> bool {
    has_client_key(config) && config.get(TLS_SAVED_KEY).and_then(|v| v.as_bool()) != Some(false)
}

fn config_text(config: &JsonValue, key: &str) -> Option<String> {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn redact_profile(mut profile: ConnectionProfile) -> ConnectionProfile {
    redact_mongo_config(&mut profile.plugin_config);
    profile.password = None;
    profile
}

fn redact_mongo_config(config: &mut JsonValue) {
    let Some(uri) = config
        .get("uri")
        .and_then(|v| v.as_str())
        .map(str::to_string)
    else {
        if let Some(obj) = config.as_object_mut() {
            obj.remove("password");
        }
        return;
    };
    let Some(creds) = crate::domain::mongo_uri::strip_mongo_userinfo(&uri) else {
        if let Some(obj) = config.as_object_mut() {
            obj.remove("password");
        }
        return;
    };
    if let Some(obj) = config.as_object_mut() {
        obj.insert("uri".into(), JsonValue::String(creds.uri));
        obj.remove("password");
        if !creds.username.is_empty() {
            obj.remove("username");
            obj.remove("user");
        }
    }
}

fn pointer_is_account(pointer: &str) -> bool {
    is_keychain_pointer(pointer) || is_legacy_keychain_pointer(pointer)
}

impl ProfileService<'_> {
    pub fn list(&self) -> AppResult<Vec<ConnectionProfile>> {
        let rows = self.db.with_conn(repo::connections::list_all)?;
        Ok(rows.into_iter().map(redact_profile).collect())
    }

    pub fn get(&self, id: &str) -> AppResult<ConnectionProfile> {
        self.db
            .with_conn(|c| repo::connections::get_by_id(c, id))
            .map(redact_profile)
    }

    /// Keychain write succeeds, then the SQLite pointer is committed.
    /// A failed keychain write rolls the new row back.
    pub fn save(&self, mut input: SaveConnectionInput) -> AppResult<ConnectionProfile> {
        let copy_from = input
            .copy_secret_from
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let mongo_password = take_mongo_credentials(&mut input);
        let mut incoming = incoming_password(&input);
        if incoming.is_none() {
            incoming = mongo_password;
        }
        if let Some(from) = copy_from.as_deref() {
            if is_mongo(&input.engine) && config_username(&input.plugin_config).is_none() {
                if let Some(user) = self.mongo_username(from)? {
                    if let Some(obj) = input.plugin_config.as_object_mut() {
                        obj.insert("username".into(), JsonValue::String(user));
                    }
                }
            }
        }
        let incoming_ssh = input
            .ssh_password
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let clear = input.clear_secret;
        let clear_ssh = input.clear_ssh_secret;
        let incoming_tls = input
            .tls_key_passphrase
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        input.clear_secret = false;
        input.clear_ssh_secret = false;
        let creating = input
            .id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none();
        let ssh_active = input.ssh_config.is_active();
        let mut input = strip_plugin_password(input);
        let install = self.install_id()?;
        // The edit form shows the URI without userinfo. Blank keeps the user.
        if !creating && is_mongo(&input.engine) && config_username(&input.plugin_config).is_none() {
            let stored = input
                .id
                .as_deref()
                .and_then(|id| {
                    self.db
                        .with_conn(|c| repo::connections::get_by_id(c, id))
                        .ok()
                })
                .and_then(|p| config_username(&p.plugin_config));
            if let (Some(user), Some(obj)) = (stored, input.plugin_config.as_object_mut()) {
                obj.insert("username".into(), JsonValue::String(user));
            }
        }
        let tls_saved = self.tls_marker(
            input.id.as_deref().filter(|_| !creating),
            incoming_tls.is_some(),
            copy_from.as_deref(),
        );
        let client_key = has_client_key(&input.plugin_config);
        if let Some(obj) = input.plugin_config.as_object_mut() {
            if client_key {
                obj.insert(TLS_SAVED_KEY.into(), JsonValue::Bool(tls_saved));
            } else {
                obj.remove(TLS_SAVED_KEY);
            }
        }

        // Updates write the keychain before the row changes. A failed write
        // leaves the previous pointer and the previous row alone.
        if !creating {
            let id = input.id.clone().unwrap_or_default();
            self.persist_secrets(
                &install,
                &id,
                ssh_active,
                clear,
                clear_ssh,
                incoming.as_deref(),
                incoming_ssh.as_deref(),
                incoming_tls.as_deref(),
                copy_from.as_deref(),
            )?;
        }

        let saved = self.db.with_conn(|c| repo::connections::save(c, input))?;
        if creating {
            if let Err(err) = self.persist_secrets(
                &install,
                &saved.id,
                ssh_active,
                clear,
                clear_ssh,
                incoming.as_deref(),
                incoming_ssh.as_deref(),
                incoming_tls.as_deref(),
                copy_from.as_deref(),
            ) {
                let _ = self
                    .db
                    .with_conn(|c| repo::connections::delete(c, &saved.id));
                let _ = self
                    .secrets
                    .delete_named(&scoped_secret_account(&install, &saved.id));
                let _ = self
                    .secrets
                    .delete_named(&scoped_ssh_secret_account(&install, &saved.id));
                let _ = self
                    .secrets
                    .delete_named(&scoped_tls_secret_account(&install, &saved.id));
                return Err(err);
            }
        }
        let saved = self
            .db
            .with_conn(|c| repo::connections::get_by_id(c, &saved.id))?;
        Ok(redact_profile(saved))
    }

    fn persist_secrets(
        &self,
        install: &str,
        id: &str,
        ssh_active: bool,
        clear: bool,
        clear_ssh: bool,
        incoming: Option<&str>,
        incoming_ssh: Option<&str>,
        incoming_tls: Option<&str>,
        copy_from: Option<&str>,
    ) -> AppResult<()> {
        self.store_password(install, id, clear, incoming, copy_from)?;
        self.store_ssh(install, id, ssh_active, clear_ssh, incoming_ssh, copy_from)?;
        self.store_tls(install, id, incoming_tls, copy_from)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let id = id.trim();
        if id.is_empty() {
            return Err(AppError::Invalid("connection id is required".into()));
        }
        let install = self.install_id()?;
        let password_ptr = self
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, id))
            .ok()
            .flatten();
        let ssh_ptr = self
            .db
            .with_conn(|c| repo::connections::get_ssh_secret_ref(c, id))
            .ok()
            .flatten();
        self.delete_account(password_ptr.as_deref())?;
        self.delete_account(ssh_ptr.as_deref())?;
        for account in [
            scoped_secret_account(&install, id),
            scoped_ssh_secret_account(&install, id),
            scoped_tls_secret_account(&install, id),
            user_secret_account(&scoped_secret_account(&install, id)),
            secret_account(id),
            ssh_secret_account(id),
            tls_secret_account(id),
            user_secret_account(&secret_account(id)),
        ] {
            self.secrets.delete_named(&account)?;
        }
        self.db.with_conn(|c| repo::connections::delete(c, id))
    }

    /// Reads the keychain only for a profile with a client key whose
    /// passphrase was saved.
    pub fn resolve_tls_secret(&self, id: &str) -> AppResult<Option<String>> {
        let id = require_id(id)?;
        let profile = self.db.with_conn(|c| repo::connections::get_by_id(c, id))?;
        if !tls_passphrase_saved(&profile.plugin_config) {
            return Ok(None);
        }
        let install = self.install_id()?;
        let scoped = scoped_tls_secret_account(&install, id);
        if let Some(secret) = self.secrets.get_named(&scoped)? {
            return Ok(Some(secret));
        }
        // `conn.<id>.tls` belongs to this row only when the row still points
        // at the legacy id account. A fresh database has a null pointer.
        let password_ptr = self
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, id))?;
        let ssh_ptr = self
            .db
            .with_conn(|c| repo::connections::get_ssh_secret_ref(c, id))?;
        let owns_legacy = password_ptr.as_deref() == Some(secret_account(id).as_str())
            || ssh_ptr.as_deref() == Some(ssh_secret_account(id).as_str());
        if !owns_legacy {
            return Ok(None);
        }
        let Some(secret) = self.secrets.get_named(&tls_secret_account(id))? else {
            return Ok(None);
        };
        self.secrets.set_named(&scoped, &secret)?;
        Ok(Some(secret))
    }

    pub fn resolve_ssh_secret(&self, id: &str) -> AppResult<Option<String>> {
        let id = require_id(id)?;
        let Some(pointer) = self
            .db
            .with_conn(|c| repo::connections::get_ssh_secret_ref(c, id))?
        else {
            return Ok(None);
        };
        if !pointer_is_account(&pointer) {
            return Ok(None);
        }
        self.secrets.get_named(&pointer)
    }

    pub fn touch_connected(&self, id: &str) -> AppResult<ConnectionProfile> {
        self.db
            .with_conn(|c| repo::connections::touch_last_connected(c, id))
            .map(redact_profile)
    }

    /// Read the keychain only when `secret_ref` is a pointer. A null pointer
    /// does not look up `conn.<id>`.
    pub fn resolve_password(&self, id: &str) -> AppResult<Option<String>> {
        let id = require_id(id)?;
        let Some(raw) = self
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, id))?
        else {
            return Ok(None);
        };
        if is_placeholder_secret_ref(&raw) {
            self.db
                .with_conn(|c| repo::connections::set_secret_ref(c, id, None))?;
            return Ok(None);
        }
        if is_legacy_keychain_pointer(&raw) {
            let Some(pw) = self.secrets.get_named(&raw)? else {
                return Ok(None);
            };
            self.promote_secret(id, &pw)?;
            return Ok(Some(pw));
        }
        if is_keychain_pointer(&raw) {
            return self.secrets.get_named(&raw);
        }
        if is_legacy_plaintext_secret(&raw) {
            return self.migrate_plaintext(id, &raw);
        }
        Ok(None)
    }

    /// One-time cleanup of plaintext and placeholder `secret_ref` rows and
    /// Mongo URI userinfo. Never reads the keychain: a row that already holds
    /// a pointer is left for connect. Runs until it succeeds once.
    pub fn migrate_plaintext_secrets(&self) -> AppResult<()> {
        if self
            .db
            .with_conn(|c| repo::settings::get(c, SECRETS_MIGRATED_KEY))?
            .is_some()
        {
            return Ok(());
        }
        let rows = self.db.with_conn(repo::connections::list_secret_refs)?;
        let mut clean = true;
        for (id, raw) in rows {
            let Some(raw) = raw else { continue };
            clean &= self.migrate_row(&id, &raw).is_ok();
        }
        self.migrate_mongo_uris()?;
        if clean {
            self.db
                .with_conn(|c| repo::settings::set(c, SECRETS_MIGRATED_KEY, "1"))?;
        }
        Ok(())
    }

    /// Mongo username. Older saves kept it in a `<pointer>.user` keychain
    /// item; the first read moves it into `plugin_config` and deletes the item.
    pub(crate) fn mongo_username(&self, id: &str) -> AppResult<Option<String>> {
        let id = require_id(id)?;
        let profile = self.db.with_conn(|c| repo::connections::get_by_id(c, id))?;
        if !is_mongo(&profile.engine) {
            return Ok(None);
        }
        if let Some(user) = config_username(&profile.plugin_config) {
            return Ok(Some(user));
        }
        let Some(pointer) = self
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, id))?
            .filter(|p| pointer_is_account(p))
        else {
            return Ok(None);
        };
        let account = user_secret_account(&pointer);
        let Some(user) = self.secrets.get_named(&account)? else {
            return Ok(None);
        };
        let mut cfg = profile.plugin_config;
        if let Some(obj) = cfg.as_object_mut() {
            obj.insert("username".into(), JsonValue::String(user.clone()));
        }
        self.db
            .with_conn(|c| repo::connections::set_plugin_config(c, id, &cfg))?;
        let _ = self.secrets.delete_named(&account);
        Ok(Some(user))
    }

    fn install_id(&self) -> AppResult<String> {
        if let Some(row) = self
            .db
            .with_conn(|c| repo::settings::get(c, INSTALL_ID_KEY))?
        {
            if Uuid::parse_str(row.value.trim()).is_ok() {
                return Ok(row.value);
            }
        }
        let id = Uuid::new_v4().to_string();
        self.db
            .with_conn(|c| repo::settings::set(c, INSTALL_ID_KEY, &id))?;
        Ok(id)
    }

    fn migrate_row(&self, id: &str, raw: &str) -> AppResult<()> {
        if is_placeholder_secret_ref(raw) {
            self.db
                .with_conn(|c| repo::connections::set_secret_ref(c, id, None))?;
        } else if is_legacy_plaintext_secret(raw) {
            self.migrate_plaintext(id, raw)?;
        }
        Ok(())
    }

    /// Whether the saved row will hold a TLS passphrase after this save.
    fn tls_marker(
        &self,
        existing_id: Option<&str>,
        incoming: bool,
        copy_from: Option<&str>,
    ) -> bool {
        if incoming {
            return true;
        }
        if let Some(id) = existing_id {
            return self
                .db
                .with_conn(|c| repo::connections::get_by_id(c, id))
                .is_ok_and(|p| tls_passphrase_saved(&p.plugin_config));
        }
        copy_from.is_some_and(|from| {
            self.db
                .with_conn(|c| repo::connections::get_by_id(c, from))
                .is_ok_and(|p| tls_passphrase_saved(&p.plugin_config))
        })
    }

    fn migrate_plaintext(&self, id: &str, plaintext: &str) -> AppResult<Option<String>> {
        self.promote_secret(id, plaintext)?;
        Ok(Some(plaintext.to_string()))
    }

    fn promote_secret(&self, id: &str, pw: &str) -> AppResult<()> {
        let install = self.install_id()?;
        let pointer = scoped_secret_account(&install, id);
        self.secrets.set_named(&pointer, pw)?;
        self.db
            .with_conn(|c| repo::connections::set_secret_ref(c, id, Some(&pointer)))?;
        Ok(())
    }

    fn migrate_mongo_uris(&self) -> AppResult<()> {
        let profiles = self.db.with_conn(repo::connections::list_all)?;
        let install = self.install_id()?;
        for profile in profiles {
            if mystique_connector_api::runtime_id(&profile.engine) != Some("mongodb") {
                continue;
            }
            let Some(uri) = config_text(&profile.plugin_config, "uri") else {
                continue;
            };
            let Some(creds) = crate::domain::mongo_uri::strip_mongo_userinfo(&uri) else {
                continue;
            };
            if creds.password.is_empty() && creds.username.is_empty() {
                continue;
            }
            let existing = self
                .db
                .with_conn(|c| repo::connections::get_secret_ref(c, &profile.id))?;
            let account = if existing.as_deref().is_some_and(pointer_is_account) {
                existing.clone().unwrap_or_default()
            } else if creds.password.is_empty() {
                String::new()
            } else {
                scoped_secret_account(&install, &profile.id)
            };
            if !creds.password.is_empty() && !account.is_empty() {
                self.secrets.set_named(&account, &creds.password)?;
                if existing.as_deref() != Some(account.as_str()) {
                    self.db.with_conn(|c| {
                        repo::connections::set_secret_ref(c, &profile.id, Some(&account))
                    })?;
                }
            }
            let mut cfg = profile.plugin_config.clone();
            if let Some(obj) = cfg.as_object_mut() {
                obj.insert("uri".into(), JsonValue::String(creds.uri));
                obj.remove("password");
                if !creds.username.is_empty() {
                    obj.remove("user");
                    obj.insert("username".into(), JsonValue::String(creds.username));
                }
            }
            self.db
                .with_conn(|c| repo::connections::set_plugin_config(c, &profile.id, &cfg))?;
        }
        Ok(())
    }

    fn store_password(
        &self,
        install: &str,
        id: &str,
        clear: bool,
        incoming: Option<&str>,
        copy_from: Option<&str>,
    ) -> AppResult<()> {
        let existing = self
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, id))?;
        if clear {
            self.delete_account(existing.as_deref())?;
            self.secrets
                .delete_named(&scoped_secret_account(install, id))?;
            self.secrets
                .delete_named(&user_secret_account(&scoped_secret_account(install, id)))?;
            self.secrets.delete_named(&secret_account(id))?;
            self.secrets
                .delete_named(&user_secret_account(&secret_account(id)))?;
            self.db
                .with_conn(|c| repo::connections::set_secret_ref(c, id, None))?;
            return Ok(());
        }
        let account = scoped_secret_account(install, id);
        if let Some(pw) = incoming {
            self.secrets.set_named(&account, pw)?;
            self.db
                .with_conn(|c| repo::connections::set_secret_ref(c, id, Some(&account)))?;
            if let Some(old) = existing.as_deref() {
                if old != account {
                    let _ = self.secrets.delete_named(old);
                    let _ = self.secrets.delete_named(&user_secret_account(old));
                }
            }
            return Ok(());
        }
        if let Some(from) = copy_from {
            if let Some(ptr) = self
                .db
                .with_conn(|c| repo::connections::get_secret_ref(c, from))?
            {
                if pointer_is_account(&ptr) {
                    if let Some(secret) = self.secrets.get_named(&ptr)? {
                        self.secrets.set_named(&account, &secret)?;
                        self.db.with_conn(|c| {
                            repo::connections::set_secret_ref(c, id, Some(&account))
                        })?;
                    }
                }
            }
        }
        Ok(())
    }

    fn store_ssh(
        &self,
        install: &str,
        id: &str,
        ssh_active: bool,
        clear: bool,
        incoming: Option<&str>,
        copy_from: Option<&str>,
    ) -> AppResult<()> {
        let existing = self
            .db
            .with_conn(|c| repo::connections::get_ssh_secret_ref(c, id))?;
        if clear || !ssh_active {
            self.delete_account(existing.as_deref())?;
            self.secrets
                .delete_named(&scoped_ssh_secret_account(install, id))?;
            self.secrets.delete_named(&ssh_secret_account(id))?;
            self.db
                .with_conn(|c| repo::connections::set_ssh_secret_ref(c, id, None))?;
            return Ok(());
        }
        let account = scoped_ssh_secret_account(install, id);
        if let Some(pw) = incoming {
            self.secrets.set_named(&account, pw)?;
            self.db
                .with_conn(|c| repo::connections::set_ssh_secret_ref(c, id, Some(&account)))?;
            if let Some(old) = existing.as_deref() {
                if old != account {
                    let _ = self.secrets.delete_named(old);
                }
            }
            return Ok(());
        }
        if let Some(from) = copy_from {
            if let Some(ptr) = self
                .db
                .with_conn(|c| repo::connections::get_ssh_secret_ref(c, from))?
            {
                if pointer_is_account(&ptr) {
                    if let Some(secret) = self.secrets.get_named(&ptr)? {
                        self.secrets.set_named(&account, &secret)?;
                        self.db.with_conn(|c| {
                            repo::connections::set_ssh_secret_ref(c, id, Some(&account))
                        })?;
                    }
                }
            }
        }
        Ok(())
    }

    fn store_tls(
        &self,
        install: &str,
        id: &str,
        incoming: Option<&str>,
        copy_from: Option<&str>,
    ) -> AppResult<()> {
        let account = scoped_tls_secret_account(install, id);
        if let Some(pw) = incoming {
            self.secrets.set_named(&account, pw)?;
            return Ok(());
        }
        let Some(from) = copy_from else {
            return Ok(());
        };
        if let Some(secret) = self.resolve_tls_secret(from)? {
            self.secrets.set_named(&account, &secret)?;
        }
        Ok(())
    }

    fn delete_account(&self, pointer: Option<&str>) -> AppResult<()> {
        let Some(pointer) = pointer.filter(|p| pointer_is_account(p)) else {
            return Ok(());
        };
        self.secrets.delete_named(pointer)?;
        self.secrets.delete_named(&user_secret_account(pointer))?;
        Ok(())
    }
}

fn require_id(id: &str) -> AppResult<&str> {
    let id = id.trim();
    if id.is_empty() {
        return Err(AppError::Invalid("connection id is required".into()));
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::AppServices;
    use crate::connectors::ConnectionManager;
    use crate::storage::AppDatabase;
    use serde_json::json;

    fn svc() -> AppServices {
        AppServices::with_secrets(
            AppDatabase::open_in_memory().unwrap(),
            ConnectionManager::bootstrap(),
            SecretStore::memory(),
        )
    }

    fn input(name: &str, password: Option<&str>) -> SaveConnectionInput {
        SaveConnectionInput {
            id: None,
            name: name.into(),
            engine: "postgres".into(),
            color_id: "teal".into(),
            environment: "local".into(),
            read_only: true,
            notes: None,
            plugin_config: json!({"host": "127.0.0.1", "database": "app"}),
            password: password.map(|s| s.into()),
            secret_ref: None,
            clear_secret: false,
            copy_secret_from: None,
            favorite: None,
            ssh_config: crate::domain::SshConfig::default(),
            ssh_password: None,
            clear_ssh_secret: false,
            tls_key_passphrase: None,
            query_timeout_secs: None,
        }
    }

    #[test]
    fn save_puts_secret_in_store_not_profile() {
        let app = svc();
        let saved = app.profiles().save(input("pg", Some("s3cret"))).unwrap();
        assert!(saved.password_set);
        assert!(saved.password.is_none());
        assert_eq!(
            app.profiles()
                .resolve_password(&saved.id)
                .unwrap()
                .as_deref(),
            Some("s3cret")
        );
        let raw = app
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, &saved.id))
            .unwrap();
        let install = install_of(&app);
        assert_eq!(
            raw.as_deref(),
            Some(scoped_secret_account(&install, &saved.id).as_str())
        );
    }

    #[test]
    fn get_list_never_include_password() {
        let app = svc();
        let saved = app.profiles().save(input("pg", Some("s3cret"))).unwrap();
        let got = app.profiles().get(&saved.id).unwrap();
        assert!(got.password.is_none());
        assert!(got.password_set);
        let listed = app.profiles().list().unwrap();
        assert!(listed[0].password.is_none());
        assert!(listed[0].password_set);
        let json = serde_json::to_value(&got).unwrap();
        assert!(json.get("password").is_none());
    }

    #[test]
    fn update_blank_keeps_secret() {
        let app = svc();
        let saved = app.profiles().save(input("pg", Some("keep-me"))).unwrap();
        let mut upd = input("pg-renamed", None);
        upd.id = Some(saved.id.clone());
        let updated = app.profiles().save(upd).unwrap();
        assert_eq!(updated.name, "pg-renamed");
        assert!(updated.password_set);
        assert_eq!(
            app.profiles()
                .resolve_password(&saved.id)
                .unwrap()
                .as_deref(),
            Some("keep-me")
        );
    }

    #[test]
    fn clear_secret_removes_keychain_item() {
        let app = svc();
        let saved = app.profiles().save(input("pg", Some("x"))).unwrap();
        let mut upd = input("pg", None);
        upd.id = Some(saved.id.clone());
        upd.clear_secret = true;
        let cleared = app.profiles().save(upd).unwrap();
        assert!(!cleared.password_set);
        assert!(app
            .profiles()
            .resolve_password(&saved.id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn migrate_plaintext_then_wipe_sqlite() {
        let app = svc();
        let saved = app.profiles().save(input("pg", None)).unwrap();
        app.db
            .with_conn(|c| repo::connections::set_secret_ref(c, &saved.id, Some("legacy-pw")))
            .unwrap();
        app.profiles().migrate_plaintext_secrets().unwrap();
        let raw = app
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, &saved.id))
            .unwrap();
        let install = install_of(&app);
        assert_eq!(
            raw.as_deref(),
            Some(scoped_secret_account(&install, &saved.id).as_str())
        );
        assert_eq!(
            app.profiles()
                .resolve_password(&saved.id)
                .unwrap()
                .as_deref(),
            Some("legacy-pw")
        );
    }

    #[test]
    fn copy_secret_on_duplicate() {
        let app = svc();
        let src = app.profiles().save(input("src", Some("cloned"))).unwrap();
        let mut dup = input("src-copy", None);
        dup.copy_secret_from = Some(src.id.clone());
        let copy = app.profiles().save(dup).unwrap();
        assert!(copy.password_set);
        assert_eq!(
            app.profiles()
                .resolve_password(&copy.id)
                .unwrap()
                .as_deref(),
            Some("cloned")
        );
    }

    #[test]
    fn ssh_secret_stays_out_of_sqlite() {
        let app = svc();
        let mut inp = input("pg-ssh", None);
        inp.ssh_config = crate::domain::SshConfig {
            enabled: true,
            host: "bastion.example".into(),
            port: 22,
            user: "ubuntu".into(),
            auth: crate::domain::SshAuth::Password,
            private_key_path: String::new(),
        };
        inp.ssh_password = Some("bastion-pw".into());
        let saved = app.profiles().save(inp).unwrap();
        assert!(saved.ssh.is_active());
        assert!(saved.ssh_password_set);
        assert_eq!(saved.ssh.host, "bastion.example");
        let raw = app
            .db
            .with_conn(|c| repo::connections::get_by_id(c, &saved.id))
            .unwrap();
        let json = serde_json::to_string(&raw).unwrap();
        assert!(!json.contains("bastion-pw"));
        assert_eq!(
            app.profiles()
                .resolve_ssh_secret(&saved.id)
                .unwrap()
                .as_deref(),
            Some("bastion-pw")
        );
    }

    fn install_of(app: &AppServices) -> String {
        app.db
            .with_conn(|c| repo::settings::get(c, "install_id"))
            .unwrap()
            .expect("install id")
            .value
    }

    #[test]
    fn null_secret_ref_does_not_read_a_planted_keychain_item() {
        let app = svc();
        let saved = app.profiles().save(input("pg", None)).unwrap();
        let install = install_of(&app);
        app.secrets.set(&saved.id, "legacy-secret").unwrap();
        app.secrets
            .set_named(&scoped_secret_account(&install, &saved.id), "scoped-secret")
            .unwrap();
        assert!(app
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, &saved.id))
            .unwrap()
            .is_none());
        assert!(app
            .profiles()
            .resolve_password(&saved.id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn mongo_uri_password_is_not_stored_in_sqlite() {
        let app = svc();
        let mut inp = input("mongo", None);
        inp.engine = "mongodb".into();
        inp.plugin_config = json!({
            "uri": "mongodb://admin:s%40cret@127.0.0.1:27017/app?authSource=admin"
        });
        let saved = app.profiles().save(inp).unwrap();
        assert!(saved.password_set);
        let uri = saved.plugin_config["uri"].as_str().unwrap();
        assert_eq!(uri, "mongodb://127.0.0.1:27017/app?authSource=admin");
        // The username is not a secret; it stays beside the stripped URI.
        assert_eq!(saved.plugin_config["username"], "admin");
        let column: String = app
            .db
            .with_conn(|c| {
                let id: i64 = saved.id.parse().unwrap();
                c.query_row(
                    "SELECT plugin_config FROM connections WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .map_err(AppError::from)
            })
            .unwrap();
        assert!(!column.contains("s%40cret"));
        assert!(!column.contains("s@cret"));
        let listed = serde_json::to_string(&app.profiles().get(&saved.id).unwrap()).unwrap();
        assert!(!listed.contains("s@cret"));
        assert!(!listed.contains("s%40cret"));
        assert_eq!(
            app.profiles()
                .resolve_password(&saved.id)
                .unwrap()
                .as_deref(),
            Some("s@cret")
        );
        let pointer = app
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, &saved.id))
            .unwrap()
            .unwrap();
        assert!(app
            .secrets
            .get_named(&user_secret_account(&pointer))
            .unwrap()
            .is_none());
    }

    #[test]
    fn documentdb_uri_uses_the_same_strip_path() {
        let app = svc();
        let mut inp = input("docdb", None);
        inp.engine = "documentdb".into();
        inp.plugin_config = json!({"uri": "mongodb://user:secret@docdb.internal:27017/app"});
        let saved = app.profiles().save(inp).unwrap();
        assert_eq!(
            saved.plugin_config["uri"].as_str(),
            Some("mongodb://docdb.internal:27017/app")
        );
        assert_eq!(
            app.profiles()
                .resolve_password(&saved.id)
                .unwrap()
                .as_deref(),
            Some("secret")
        );
        let column: String = app
            .db
            .with_conn(|c| {
                let id: i64 = saved.id.parse().unwrap();
                c.query_row(
                    "SELECT plugin_config FROM connections WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .map_err(AppError::from)
            })
            .unwrap();
        assert!(!column.contains("secret"));
    }

    #[test]
    fn failed_keychain_write_does_not_keep_a_profile_row() {
        let backend = Arc::new(Flaky {
            map: Mutex::new(HashMap::new()),
            fail: AtomicBool::new(true),
        });
        let app = AppServices::with_secrets(
            AppDatabase::open_in_memory().unwrap(),
            ConnectionManager::bootstrap(),
            SecretStore::from_backend(backend.clone()),
        );
        assert!(app.profiles().save(input("pg", Some("pw"))).is_err());
        assert!(app.profiles().list().unwrap().is_empty());

        backend.fail.store(false, Ordering::SeqCst);
        let saved = app.profiles().save(input("pg", Some("pw"))).unwrap();
        assert_eq!(app.profiles().list().unwrap().len(), 1);

        backend.fail.store(true, Ordering::SeqCst);
        let mut upd = input("renamed", Some("other"));
        upd.id = Some(saved.id.clone());
        assert!(app.profiles().save(upd).is_err());
        let got = app.profiles().get(&saved.id).unwrap();
        assert_eq!(got.name, "pg");
        assert_eq!(
            app.profiles()
                .resolve_password(&saved.id)
                .unwrap()
                .as_deref(),
            Some("pw")
        );
    }

    #[test]
    fn clear_of_a_missing_keychain_item_nulls_the_pointer() {
        let app = svc();
        let saved = app.profiles().save(input("pg", Some("x"))).unwrap();
        let pointer = app
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, &saved.id))
            .unwrap()
            .unwrap();
        app.secrets.delete_named(&pointer).unwrap();
        let mut upd = input("pg", None);
        upd.id = Some(saved.id.clone());
        upd.clear_secret = true;
        let cleared = app.profiles().save(upd).unwrap();
        assert!(!cleared.password_set);
        assert!(app
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, &saved.id))
            .unwrap()
            .is_none());
    }

    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    struct Flaky {
        map: Mutex<HashMap<String, String>>,
        fail: AtomicBool,
    }

    impl crate::secrets::SecretBackend for Flaky {
        fn get(&self, account: &str) -> AppResult<Option<String>> {
            let map = self.map.lock().unwrap_or_else(|p| p.into_inner());
            Ok(map.get(account).cloned())
        }

        fn set(&self, account: &str, secret: &str) -> AppResult<()> {
            if self.fail.load(Ordering::SeqCst) {
                return Err(AppError::SecretStore("keychain write failed".into()));
            }
            let mut map = self.map.lock().unwrap_or_else(|p| p.into_inner());
            map.insert(account.to_string(), secret.to_string());
            Ok(())
        }

        fn delete(&self, account: &str) -> AppResult<()> {
            if self.fail.load(Ordering::SeqCst) {
                return Err(AppError::SecretStore("keychain delete failed".into()));
            }
            let mut map = self.map.lock().unwrap_or_else(|p| p.into_inner());
            map.remove(account);
            Ok(())
        }
    }
}
