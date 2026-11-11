use serde_json::Value as JsonValue;

use crate::domain::{ConnectionProfile, SaveConnectionInput, VAULT_SECRET_REF};
use crate::error::{AppError, AppResult};
use crate::secrets::{Prompt, SecretKind, SecretOwner, SecretStore};
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

/// Non-secret marker in `plugin_config`: a TLS key passphrase is saved.
const TLS_SAVED_KEY: &str = "clientKeyPassphraseSaved";

pub struct ProfileService<'a> {
    pub(crate) db: &'a AppDatabase,
    pub(crate) secrets: &'a SecretStore,
    /// MCP binds with `Prompt::Never`.
    pub(crate) prompt: Prompt,
}

fn incoming_password(input: &SaveConnectionInput) -> Option<String> {
    input
        .password
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
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

/// Every save of a profile with a client key writes the marker.
fn tls_passphrase_saved(config: &JsonValue) -> bool {
    has_client_key(config) && config.get(TLS_SAVED_KEY).and_then(|v| v.as_bool()) == Some(true)
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

    /// The row and its envelopes go in one transaction.
    pub fn delete(&self, id: &str) -> AppResult<()> {
        let id = id.trim();
        if id.is_empty() {
            return Err(AppError::Invalid("connection id is required".into()));
        }
        self.db.with_tx(|c| {
            SecretStore::remove_owner(c, SecretOwner::Connection(id))?;
            repo::connections::delete(c, id)
        })?;
        self.secrets.session().forget(id);
        Ok(())
    }

    /// Reads only for a profile with a client key whose passphrase was saved.
    pub fn resolve_tls_secret(&self, id: &str) -> AppResult<Option<String>> {
        let id = require_id(id)?;
        let profile = self.db.with_conn(|c| repo::connections::get_by_id(c, id))?;
        if !tls_passphrase_saved(&profile.plugin_config) {
            return Ok(None);
        }
        self.secrets.read(
            self.db,
            SecretOwner::Connection(id),
            SecretKind::Tls,
            self.prompt,
        )
    }

    /// Reads only when `ssh_secret_ref` is [`VAULT_SECRET_REF`].
    pub fn resolve_ssh_secret(&self, id: &str) -> AppResult<Option<String>> {
        let id = require_id(id)?;
        let pointer = self
            .db
            .with_conn(|c| repo::connections::get_ssh_secret_ref(c, id))?;
        if pointer.as_deref() != Some(VAULT_SECRET_REF) {
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

    /// Reads only when `secret_ref` is [`VAULT_SECRET_REF`]. A profile that
    /// does not save its password reads nothing.
    pub fn resolve_password(&self, id: &str) -> AppResult<Option<String>> {
        let id = require_id(id)?;
        let (save_password, pointer) = self.db.with_conn(|c| {
            Ok((
                repo::connections::get_by_id(c, id)?.save_password,
                repo::connections::get_secret_ref(c, id)?,
            ))
        })?;
        if !save_password || pointer.as_deref() != Some(VAULT_SECRET_REF) {
            return Ok(None);
        }
        self.secrets.read(
            self.db,
            SecretOwner::Connection(id),
            SecretKind::Password,
            self.prompt,
        )
    }

    /// Mongo username, kept in `plugin_config` beside the stripped URI.
    pub(crate) fn mongo_username(&self, id: &str) -> AppResult<Option<String>> {
        let id = require_id(id)?;
        let profile = self.db.with_conn(|c| repo::connections::get_by_id(c, id))?;
        if !is_mongo(&profile.engine) {
            return Ok(None);
        }
        Ok(config_username(&profile.plugin_config))
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
    use crate::secrets::counting::CountingKeySource;
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

    fn counted() -> (AppServices, Arc<CountingKeySource>) {
        let keys = Arc::new(CountingKeySource::default());
        let app = AppServices::with_secrets(
            AppDatabase::open_in_memory().unwrap(),
            ConnectionManager::bootstrap(),
            SecretStore::with_key_source(keys.clone()),
        );
        (app, keys)
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
    fn null_secret_ref_loads_no_key() {
        let (app, keys) = counted();
        let saved = app.profiles().save(input("pg", None)).unwrap();
        assert!(secret_ref(&app, &saved.id).is_none());
        assert!(app
            .profiles()
            .resolve_password(&saved.id)
            .unwrap()
            .is_none());
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
        let (app, keys) = counted();
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
