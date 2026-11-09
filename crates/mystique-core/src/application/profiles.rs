use serde_json::Value as JsonValue;

use crate::domain::{
    is_keychain_pointer, is_legacy_keychain_pointer, is_placeholder_secret_ref,
    scoped_tls_secret_account, secret_account, ssh_secret_account, tls_secret_account,
    user_secret_account, ConnectionProfile, SaveConnectionInput, VAULT_SECRET_REF,
};
use crate::error::{AppError, AppResult};
use crate::secrets::{
    install_id, is_legacy_plaintext_secret, Prompt, SecretKind, SecretOwner, SecretStore,
};
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

/// Set once the startup cleanup of placeholder rows has run.
const SECRETS_MIGRATED_KEY: &str = "secrets.startup_migration_v1";
/// Non-secret marker in `plugin_config`: a TLS key passphrase is saved.
const TLS_SAVED_KEY: &str = "clientKeyPassphraseSaved";

pub struct ProfileService<'a> {
    pub(crate) db: &'a AppDatabase,
    pub(crate) secrets: &'a SecretStore,
    /// MCP binds with `Prompt::Never`.
    pub(crate) prompt: Prompt,
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
        .filter(|s| is_legacy_plaintext_secret(s))
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

    /// Mark one database (or schema) with an environment, or clear its mark.
    pub fn set_database_environment(
        &self,
        input: &crate::domain::SetDatabaseEnvironmentInput,
    ) -> AppResult<ConnectionProfile> {
        let id = input.profile_id.trim();
        self.db
            .with_conn(|c| {
                let profile = repo::connections::get_by_id(c, id)?;
                let marks = crate::domain::with_database_environment(
                    profile.database_environments,
                    &input.database,
                    input.schema.as_deref(),
                    input.environment.as_deref(),
                )
                .map_err(crate::error::AppError::Invalid)?;
                repo::connections::set_database_environments(c, id, &marks)
            })
            .map(redact_profile)
    }

    /// The row and its secrets commit in one transaction. The vault key
    /// loads first, outside it, so a denied prompt writes nothing.
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
        let existing_id = input
            .id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let creating = existing_id.is_none();
        let ssh_active = input.ssh_config.is_active();
        let before = existing_id.as_deref().and_then(|id| {
            self.db
                .with_conn(|c| repo::connections::get_by_id(c, id))
                .ok()
        });
        // Off: the DB password is asked each session and never stored.
        let save_password = input
            .save_password
            .or(before.as_ref().map(|p| p.save_password))
            .unwrap_or(true);
        input.save_password = Some(save_password);
        if !save_password {
            incoming = None;
        }
        let mut input = strip_plugin_password(input);
        // The edit form shows the URI without userinfo. Blank keeps the user.
        if !creating && is_mongo(&input.engine) && config_username(&input.plugin_config).is_none() {
            let stored = existing_id
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
            existing_id.as_deref(),
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

        // A duplicate copies the source's secrets. They are read up front.
        let copied = |want: bool, read: &dyn Fn(&str) -> AppResult<Option<String>>| match copy_from
            .as_deref()
        {
            Some(from) if want => read(from),
            _ => Ok(None),
        };
        let password = match incoming {
            _ if !save_password => None,
            Some(pw) => Some(pw),
            None => copied(!clear, &|from| self.resolve_password(from))?,
        };
        let ssh = match incoming_ssh {
            _ if clear_ssh || !ssh_active => None,
            Some(secret) => Some(secret),
            None => copied(true, &|from| self.resolve_ssh_secret(from))?,
        };
        let tls = match incoming_tls {
            _ if !client_key => None,
            Some(pp) => Some(pp),
            None => copied(true, &|from| self.resolve_tls_secret(from))?,
        };
        let key = if password.is_some() || ssh.is_some() || tls.is_some() {
            Some(self.secrets.unlock(self.prompt)?)
        } else {
            None
        };
        let old_refs = match existing_id.as_deref() {
            Some(id) => self.secret_refs(id).unwrap_or_default(),
            None => (None, None),
        };

        let saved_id = self.db.with_tx(|c| {
            let saved = repo::connections::save(c, input)?;
            let id = saved.id.as_str();
            let owner = SecretOwner::Connection(id);
            if clear || !save_password {
                SecretStore::remove(c, owner, SecretKind::Password)?;
                repo::connections::set_secret_ref(c, id, None)?;
            } else if let (Some(key), Some(pw)) = (&key, &password) {
                self.secrets
                    .write(c, key, owner, SecretKind::Password, pw)?;
                repo::connections::set_secret_ref(c, id, Some(VAULT_SECRET_REF))?;
            }
            if clear_ssh || !ssh_active {
                SecretStore::remove(c, owner, SecretKind::Ssh)?;
                repo::connections::set_ssh_secret_ref(c, id, None)?;
            } else if let (Some(key), Some(secret)) = (&key, &ssh) {
                self.secrets.write(c, key, owner, SecretKind::Ssh, secret)?;
                repo::connections::set_ssh_secret_ref(c, id, Some(VAULT_SECRET_REF))?;
            }
            if !client_key {
                SecretStore::remove(c, owner, SecretKind::Tls)?;
            } else if let (Some(key), Some(pp)) = (&key, &tls) {
                self.secrets.write(c, key, owner, SecretKind::Tls, pp)?;
            }
            Ok(saved.id)
        })?;

        // Old keychain items the row no longer points at.
        let new_refs = self.secret_refs(&saved_id)?;
        if old_refs.0 != new_refs.0 {
            self.drop_items(old_refs.0.as_deref());
        }
        if old_refs.1 != new_refs.1 {
            self.drop_items(old_refs.1.as_deref());
        }
        let saved = self
            .db
            .with_conn(|c| repo::connections::get_by_id(c, &saved_id))?;
        if let Some(before) = &before {
            if save_password || !same_target(before, &saved) {
                self.secrets.session().forget(&saved_id);
            }
        }
        Ok(redact_profile(saved))
    }

    /// The row and its envelopes go in one transaction. Old keychain items
    /// are deleted after, without UI.
    pub fn delete(&self, id: &str) -> AppResult<()> {
        let id = id.trim();
        if id.is_empty() {
            return Err(AppError::Invalid("connection id is required".into()));
        }
        let (password_ptr, ssh_ptr) = self.secret_refs(id).unwrap_or_default();
        self.db.with_tx(|c| {
            SecretStore::remove_owner(c, SecretOwner::Connection(id))?;
            repo::connections::delete(c, id)
        })?;
        self.secrets.session().forget(id);
        self.drop_items(password_ptr.as_deref());
        self.drop_items(ssh_ptr.as_deref());
        Ok(())
    }

    /// Reads only for a profile with a client key whose passphrase was saved.
    pub fn resolve_tls_secret(&self, id: &str) -> AppResult<Option<String>> {
        let id = require_id(id)?;
        let profile = self.db.with_conn(|c| repo::connections::get_by_id(c, id))?;
        if !tls_passphrase_saved(&profile.plugin_config) {
            return Ok(None);
        }
        let owner = SecretOwner::Connection(id);
        if let Some(pp) = self
            .secrets
            .read(self.db, owner, SecretKind::Tls, self.prompt)?
        {
            return Ok(Some(pp));
        }
        if !self.secrets.legacy_enabled() {
            return Ok(None);
        }
        self.migrate_connection(id)?;
        self.secrets
            .read(self.db, owner, SecretKind::Tls, self.prompt)
    }

    pub fn resolve_ssh_secret(&self, id: &str) -> AppResult<Option<String>> {
        let id = require_id(id)?;
        let Some(pointer) = self
            .db
            .with_conn(|c| repo::connections::get_ssh_secret_ref(c, id))?
        else {
            return Ok(None);
        };
        if pointer_is_account(&pointer) {
            if !self.secrets.legacy_enabled() {
                return Ok(None);
            }
            self.migrate_connection(id)?;
        } else if pointer != VAULT_SECRET_REF {
            return Ok(None);
        }
        if self.secret_refs(id)?.1.as_deref() != Some(VAULT_SECRET_REF) {
            return Ok(None);
        }
        self.secrets.read(
            self.db,
            SecretOwner::Connection(id),
            SecretKind::Ssh,
            self.prompt,
        )
    }

    pub fn touch_connected(&self, id: &str) -> AppResult<ConnectionProfile> {
        self.db
            .with_conn(|c| repo::connections::touch_last_connected(c, id))
            .map(redact_profile)
    }

    /// Read only what `secret_ref` points at. A null pointer reads nothing,
    /// so a stale `conn.<id>` item is never picked up.
    /// A profile that does not save its password reads nothing.
    pub fn resolve_password(&self, id: &str) -> AppResult<Option<String>> {
        let id = require_id(id)?;
        if !self
            .db
            .with_conn(|c| repo::connections::get_by_id(c, id))?
            .save_password
        {
            return Ok(None);
        }
        let mut raw = self
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, id))?;
        if raw.as_deref().is_some_and(is_placeholder_secret_ref) {
            self.db
                .with_conn(|c| repo::connections::set_secret_ref(c, id, None))?;
            raw = None;
        }
        // Plaintext from older builds, in the row or in a Mongo URI.
        if raw.as_deref().is_none_or(is_legacy_plaintext_secret) {
            return self.seal_plaintext_row(id);
        }
        if raw.as_deref().is_some_and(pointer_is_account) {
            self.migrate_connection(id)?;
        }
        if self.secret_refs(id)?.0.as_deref() != Some(VAULT_SECRET_REF) {
            return Ok(None);
        }
        self.secrets.read(
            self.db,
            SecretOwner::Connection(id),
            SecretKind::Password,
            self.prompt,
        )
    }

    /// One-time startup cleanup: placeholder pointers become null. Loads no
    /// key and reads no keychain item. Plaintext rows wait for the vault:
    /// their first connect seals them, or the first unlock does.
    pub fn startup_cleanup(&self) -> AppResult<()> {
        if self
            .db
            .with_conn(|c| repo::settings::get(c, SECRETS_MIGRATED_KEY))?
            .is_some()
        {
            return Ok(());
        }
        let rows = self.db.with_conn(repo::connections::list_secret_refs)?;
        for (id, raw) in rows {
            if raw.as_deref().is_some_and(is_placeholder_secret_ref) {
                self.db
                    .with_conn(|c| repo::connections::set_secret_ref(c, &id, None))?;
            }
        }
        self.db
            .with_conn(|c| repo::settings::set(c, SECRETS_MIGRATED_KEY, "1"))?;
        Ok(())
    }

    /// Seal every plaintext row and Mongo URI password into the vault. Runs
    /// after the first unlock, when the key is already loaded.
    pub(crate) fn seal_plaintext_rows(&self) -> AppResult<()> {
        let rows = self.db.with_conn(repo::connections::list_secret_refs)?;
        let mut first_err = None;
        for (id, _) in rows {
            if let Err(e) = self.seal_plaintext_row(&id) {
                first_err.get_or_insert(e);
            }
        }
        first_err.map_or(Ok(()), Err)
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
        if !self.secrets.legacy_enabled() {
            return Ok(None);
        }
        let Some(pointer) = self
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, id))?
            .filter(|p| pointer_is_account(p))
        else {
            return Ok(None);
        };
        let account = user_secret_account(&pointer);
        let Some(user) = self.secrets.legacy_get(&account, self.prompt)? else {
            return Ok(None);
        };
        let mut cfg = profile.plugin_config;
        if let Some(obj) = cfg.as_object_mut() {
            obj.insert("username".into(), JsonValue::String(user.clone()));
        }
        self.db
            .with_conn(|c| repo::connections::set_plugin_config(c, id, &cfg))?;
        self.secrets.legacy_delete(&account);
        Ok(Some(user))
    }

    fn secret_refs(&self, id: &str) -> AppResult<(Option<String>, Option<String>)> {
        self.db.with_conn(|c| {
            Ok((
                repo::connections::get_secret_ref(c, id)?,
                repo::connections::get_ssh_secret_ref(c, id)?,
            ))
        })
    }

    /// Move a row's old keychain items into the vault. Each item is read
    /// once; the envelopes and pointers commit together; then the items go.
    fn migrate_connection(&self, id: &str) -> AppResult<()> {
        let profile = self.db.with_conn(|c| repo::connections::get_by_id(c, id))?;
        let owner = SecretOwner::Connection(id);
        let (password_ptr, ssh_ptr, install, tls_in_vault) = self.db.with_conn(|c| {
            Ok((
                repo::connections::get_secret_ref(c, id)?.filter(|p| pointer_is_account(p)),
                repo::connections::get_ssh_secret_ref(c, id)?.filter(|p| pointer_is_account(p)),
                install_id(c)?,
                SecretStore::has(c, owner, SecretKind::Tls)?,
            ))
        })?;
        let wants_tls = tls_passphrase_saved(&profile.plugin_config) && !tls_in_vault;
        if password_ptr.is_none() && ssh_ptr.is_none() && !wants_tls {
            return Ok(());
        }
        let read = |account: Option<&str>| match account {
            Some(account) => self.secrets.legacy_get(account, self.prompt),
            None => Ok(None),
        };
        let password = read(password_ptr.as_deref())?;
        let ssh = read(ssh_ptr.as_deref())?;
        let user_account = password_ptr
            .as_deref()
            .filter(|_| {
                is_mongo(&profile.engine) && config_username(&profile.plugin_config).is_none()
            })
            .map(user_secret_account);
        let user = read(user_account.as_deref())?;
        let mut tls = None;
        if wants_tls {
            // `conn.<id>.tls` belongs to this row only while it still points
            // at the old id account. A fresh database has a null pointer.
            let owns_old_id = password_ptr.as_deref() == Some(secret_account(id).as_str())
                || ssh_ptr.as_deref() == Some(ssh_secret_account(id).as_str());
            let mut accounts = vec![scoped_tls_secret_account(&install, id)];
            if owns_old_id {
                accounts.push(tls_secret_account(id));
            }
            for account in accounts {
                if let Some(pp) = read(Some(&account))? {
                    tls = Some((account, pp));
                    break;
                }
            }
        }

        let key = if password.is_some() || ssh.is_some() || tls.is_some() {
            Some(self.secrets.unlock(self.prompt)?)
        } else {
            None
        };
        self.db.with_tx(|c| {
            let mut cfg = profile.plugin_config.clone();
            if let (Some(key), Some(pw)) = (&key, &password) {
                self.secrets
                    .write(c, key, owner, SecretKind::Password, pw)?;
                repo::connections::set_secret_ref(c, id, Some(VAULT_SECRET_REF))?;
            }
            if let (Some(key), Some(secret)) = (&key, &ssh) {
                self.secrets.write(c, key, owner, SecretKind::Ssh, secret)?;
                repo::connections::set_ssh_secret_ref(c, id, Some(VAULT_SECRET_REF))?;
            }
            match (&key, &tls) {
                (Some(key), Some((_, pp))) => {
                    self.secrets.write(c, key, owner, SecretKind::Tls, pp)?;
                }
                // Nothing saved anywhere: stop looking on every connect.
                _ if wants_tls => {
                    if let Some(obj) = cfg.as_object_mut() {
                        obj.insert(TLS_SAVED_KEY.into(), JsonValue::Bool(false));
                    }
                }
                _ => {}
            }
            if let (Some(user), Some(obj)) = (&user, cfg.as_object_mut()) {
                obj.insert("username".into(), JsonValue::String(user.clone()));
            }
            if cfg != profile.plugin_config {
                repo::connections::set_plugin_config(c, id, &cfg)?;
            }
            Ok(())
        })?;

        if password.is_some() {
            self.drop_items(password_ptr.as_deref());
        } else if let (Some(account), Some(_)) = (&user_account, &user) {
            self.secrets.legacy_delete(account);
        }
        if ssh.is_some() {
            self.drop_items(ssh_ptr.as_deref());
        }
        if let Some((account, _)) = &tls {
            self.secrets.legacy_delete(account);
        }
        Ok(())
    }

    /// Plaintext from older builds: a password in `secret_ref`, or userinfo in
    /// a stored Mongo URI. Sealed into the vault; the URI keeps the username.
    fn seal_plaintext_row(&self, id: &str) -> AppResult<Option<String>> {
        let profile = self.db.with_conn(|c| repo::connections::get_by_id(c, id))?;
        let raw = self
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, id))?;
        let plaintext = raw
            .as_deref()
            .filter(|r| is_legacy_plaintext_secret(r))
            .map(str::to_string);
        let creds = config_text(&profile.plugin_config, "uri")
            .filter(|_| is_mongo(&profile.engine))
            .and_then(|uri| crate::domain::mongo_uri::strip_mongo_userinfo(&uri))
            .filter(|c| !c.password.is_empty() || !c.username.is_empty());
        let password = creds
            .as_ref()
            .map(|c| c.password.clone())
            .filter(|p| !p.is_empty())
            .or(plaintext);
        if password.is_none() && creds.is_none() {
            return Ok(None);
        }
        let key = match password {
            Some(_) => Some(self.secrets.unlock(self.prompt)?),
            None => None,
        };
        self.db.with_tx(|c| {
            if let (Some(key), Some(pw)) = (&key, &password) {
                self.secrets.write(
                    c,
                    key,
                    SecretOwner::Connection(id),
                    SecretKind::Password,
                    pw,
                )?;
                repo::connections::set_secret_ref(c, id, Some(VAULT_SECRET_REF))?;
            }
            if let Some(creds) = &creds {
                let mut cfg = profile.plugin_config.clone();
                if let Some(obj) = cfg.as_object_mut() {
                    obj.insert("uri".into(), JsonValue::String(creds.uri.clone()));
                    obj.remove("password");
                    if !creds.username.is_empty() {
                        obj.remove("user");
                        obj.insert("username".into(), JsonValue::String(creds.username.clone()));
                    }
                }
                repo::connections::set_plugin_config(c, id, &cfg)?;
            }
            Ok(())
        })?;
        if password.is_some() {
            self.drop_items(raw.as_deref());
        }
        Ok(password)
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

    /// Delete an old keychain item and its `.user` / `.tls` siblings. Never
    /// prompts; anything skipped is left for the orphan cleanup.
    fn drop_items(&self, pointer: Option<&str>) {
        let Some(pointer) = pointer.filter(|p| pointer_is_account(p)) else {
            return;
        };
        self.secrets.legacy_delete(pointer);
        self.secrets.legacy_delete(&user_secret_account(pointer));
        self.secrets.legacy_delete(&format!("{pointer}.tls"));
    }
}

/// Same host and bastion: a session password typed for one still fits.
fn same_target(before: &ConnectionProfile, after: &ConnectionProfile) -> bool {
    before.engine == after.engine
        && crate::domain::host_identity::same_connection_target(
            &before.engine,
            &before.plugin_config,
            &after.plugin_config,
        )
        && before.ssh == after.ssh
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
    use crate::secrets::counting::{CountingBackend, CountingKeySource};
    use crate::storage::AppDatabase;
    use serde_json::json;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    fn svc() -> AppServices {
        AppServices::with_secrets(
            AppDatabase::open_in_memory().unwrap(),
            ConnectionManager::bootstrap(),
            SecretStore::memory(),
        )
    }

    fn counted() -> (AppServices, Arc<CountingKeySource>, Arc<CountingBackend>) {
        let keys = Arc::new(CountingKeySource::default());
        let items = Arc::new(CountingBackend::default());
        let app = AppServices::with_secrets(
            AppDatabase::open_in_memory().unwrap(),
            ConnectionManager::bootstrap(),
            SecretStore::from_parts(keys.clone(), Some(items.clone())),
        );
        (app, keys, items)
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
            save_password: None,
        }
    }

    fn secret_ref(app: &AppServices, id: &str) -> Option<String> {
        app.db
            .with_conn(|c| repo::connections::get_secret_ref(c, id))
            .unwrap()
    }

    fn envelope(app: &AppServices, id: &str, kind: &str) -> Option<String> {
        app.db
            .with_conn(|c| repo::secrets::get(c, "connection", id, kind))
            .unwrap()
            .map(|r| r.envelope)
    }

    fn column(app: &AppServices, id: &str, col: &str) -> String {
        app.db
            .with_conn(|c| {
                let id: i64 = id.parse().unwrap();
                c.query_row(
                    &format!("SELECT {col} FROM connections WHERE id = ?1"),
                    [id],
                    |row| row.get(0),
                )
                .map_err(AppError::from)
            })
            .unwrap()
    }

    #[test]
    fn save_puts_secret_in_the_vault_not_the_profile() {
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
        assert_eq!(
            secret_ref(&app, &saved.id).as_deref(),
            Some(VAULT_SECRET_REF)
        );
        let env = envelope(&app, &saved.id, "password").expect("envelope");
        assert!(env.starts_with("v1."));
        assert!(!env.contains("s3cret"));
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
    fn clear_secret_removes_the_envelope() {
        let app = svc();
        let saved = app.profiles().save(input("pg", Some("x"))).unwrap();
        let mut upd = input("pg", None);
        upd.id = Some(saved.id.clone());
        upd.clear_secret = true;
        let cleared = app.profiles().save(upd).unwrap();
        assert!(!cleared.password_set);
        assert!(envelope(&app, &saved.id, "password").is_none());
        assert!(app
            .profiles()
            .resolve_password(&saved.id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn plaintext_is_sealed_on_first_use_not_at_startup() {
        let (app, keys, _) = counted();
        let saved = app.profiles().save(input("pg", None)).unwrap();
        app.db
            .with_conn(|c| repo::connections::set_secret_ref(c, &saved.id, Some("legacy-pw")))
            .unwrap();
        app.profiles().startup_cleanup().unwrap();
        assert_eq!(secret_ref(&app, &saved.id).as_deref(), Some("legacy-pw"));
        assert_eq!(keys.loads(), 0);
        assert_eq!(
            app.profiles()
                .resolve_password(&saved.id)
                .unwrap()
                .as_deref(),
            Some("legacy-pw")
        );
        assert_eq!(
            secret_ref(&app, &saved.id).as_deref(),
            Some(VAULT_SECRET_REF)
        );
        assert!(envelope(&app, &saved.id, "password").is_some());
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
    fn ssh_secret_stays_out_of_sqlite_plaintext() {
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
        assert!(!envelope(&app, &saved.id, "ssh")
            .unwrap()
            .contains("bastion-pw"));
        assert_eq!(
            app.profiles()
                .resolve_ssh_secret(&saved.id)
                .unwrap()
                .as_deref(),
            Some("bastion-pw")
        );
    }

    #[test]
    fn null_secret_ref_reads_no_keychain_item_and_no_key() {
        let (app, keys, items) = counted();
        let saved = app.profiles().save(input("pg", None)).unwrap();
        let install = app.db.with_conn(install_id).unwrap();
        items.plant(&secret_account(&saved.id), "legacy-secret");
        items.plant(
            &crate::domain::scoped_secret_account(&install, &saved.id),
            "scoped-secret",
        );
        assert!(secret_ref(&app, &saved.id).is_none());
        assert!(app
            .profiles()
            .resolve_password(&saved.id)
            .unwrap()
            .is_none());
        assert_eq!(items.total_reads(), 0);
        assert_eq!(keys.loads(), 0);
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
        let config = column(&app, &saved.id, "plugin_config");
        assert!(!config.contains("s%40cret"));
        assert!(!config.contains("s@cret"));
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
    }

    #[test]
    fn a_stored_mongo_uri_password_is_sealed_on_connect() {
        let app = svc();
        let mut inp = input("mongo", None);
        inp.engine = "mongodb".into();
        inp.plugin_config = json!({"uri": "mongodb://127.0.0.1:27017/app"});
        let saved = app.profiles().save(inp).unwrap();
        // How an old build left it: userinfo in the stored URI, no pointer.
        app.db
            .with_conn(|c| {
                repo::connections::set_plugin_config(
                    c,
                    &saved.id,
                    &json!({"uri": "mongodb://admin:pw@127.0.0.1:27017/app"}),
                )
            })
            .unwrap();
        assert_eq!(
            app.profiles()
                .resolve_password(&saved.id)
                .unwrap()
                .as_deref(),
            Some("pw")
        );
        let config = column(&app, &saved.id, "plugin_config");
        assert!(!config.contains(":pw@"));
        assert!(config.contains("\"username\":\"admin\""));
        assert_eq!(
            secret_ref(&app, &saved.id).as_deref(),
            Some(VAULT_SECRET_REF)
        );
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
        assert!(!column(&app, &saved.id, "plugin_config").contains("secret"));
    }

    #[test]
    fn a_denied_vault_writes_no_profile_row() {
        let (app, keys, _) = counted();
        keys.deny.store(true, Ordering::SeqCst);
        let err = app.profiles().save(input("pg", Some("pw"))).unwrap_err();
        assert!(err.to_string().contains("Retry keychain"), "{err}");
        assert!(app.profiles().list().unwrap().is_empty());
        // A profile without secrets needs no key.
        app.profiles().save(input("no-pw", None)).unwrap();

        keys.deny.store(false, Ordering::SeqCst);
        assert_eq!(app.secrets.retry_unlock().state, "unlocked");
        app.profiles().save(input("pg", Some("pw"))).unwrap();
        assert_eq!(app.profiles().list().unwrap().len(), 2);
    }

    fn fail_on(app: &AppServices, event: &str) {
        app.db
            .with_conn(|c| {
                c.execute_batch(&format!(
                    "CREATE TRIGGER fail_secrets BEFORE {event} ON secrets
                     BEGIN SELECT RAISE(ABORT, 'injected'); END;"
                ))
                .map_err(AppError::from)
            })
            .unwrap();
    }

    fn stop_failing(app: &AppServices) {
        app.db
            .with_conn(|c| {
                c.execute_batch("DROP TRIGGER fail_secrets")
                    .map_err(AppError::from)
            })
            .unwrap();
    }

    #[test]
    fn save_and_delete_are_one_transaction() {
        let app = svc();
        // Create: the row is rolled back with the envelope.
        fail_on(&app, "INSERT");
        assert!(app.profiles().save(input("pg", Some("pw"))).is_err());
        assert!(app.profiles().list().unwrap().is_empty());
        stop_failing(&app);
        let saved = app.profiles().save(input("pg", Some("pw"))).unwrap();

        // Update: name, pointer and envelope all stay as they were.
        fail_on(&app, "UPDATE");
        let mut upd = input("renamed", Some("other"));
        upd.id = Some(saved.id.clone());
        assert!(app.profiles().save(upd).is_err());
        assert_eq!(app.profiles().get(&saved.id).unwrap().name, "pg");
        assert_eq!(
            app.profiles()
                .resolve_password(&saved.id)
                .unwrap()
                .as_deref(),
            Some("pw")
        );
        stop_failing(&app);

        // Delete: the row survives when its envelopes cannot be removed.
        fail_on(&app, "DELETE");
        assert!(app.profiles().delete(&saved.id).is_err());
        assert_eq!(app.profiles().list().unwrap().len(), 1);
        assert!(envelope(&app, &saved.id, "password").is_some());
        stop_failing(&app);

        app.profiles().delete(&saved.id).unwrap();
        assert!(app.profiles().list().unwrap().is_empty());
        assert!(envelope(&app, &saved.id, "password").is_none());
    }

    #[test]
    fn clear_of_a_missing_envelope_nulls_the_pointer() {
        let app = svc();
        let saved = app.profiles().save(input("pg", Some("x"))).unwrap();
        app.db
            .with_conn(|c| repo::secrets::delete(c, "connection", &saved.id, "password"))
            .unwrap();
        let mut upd = input("pg", None);
        upd.id = Some(saved.id.clone());
        upd.clear_secret = true;
        let cleared = app.profiles().save(upd).unwrap();
        assert!(!cleared.password_set);
        assert!(secret_ref(&app, &saved.id).is_none());
    }

    #[test]
    fn removing_the_client_key_drops_the_tls_envelope() {
        let app = svc();
        let mut inp = input("tls", None);
        inp.plugin_config["clientKeyPath"] = json!("/certs/client.key");
        inp.tls_key_passphrase = Some("pp".into());
        let saved = app.profiles().save(inp).unwrap();
        assert!(envelope(&app, &saved.id, "tls").is_some());
        let mut upd = input("tls", None);
        upd.id = Some(saved.id.clone());
        let updated = app.profiles().save(upd).unwrap();
        assert!(updated.plugin_config.get(TLS_SAVED_KEY).is_none());
        assert!(envelope(&app, &saved.id, "tls").is_none());
    }
}
