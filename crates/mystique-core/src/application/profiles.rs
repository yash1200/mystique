use crate::domain::{
    is_keychain_pointer, is_placeholder_secret_ref, secret_account, ConnectionProfile,
    SaveConnectionInput,
};
use crate::error::{AppError, AppResult};
use crate::secrets::{is_legacy_plaintext_secret, SecretStore};
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

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
    }
    input
}

impl ProfileService<'_> {
    pub fn list(&self) -> AppResult<Vec<ConnectionProfile>> {
        self.db.with_conn(repo::connections::list_all)
    }

    pub fn get(&self, id: &str) -> AppResult<ConnectionProfile> {
        self.db.with_conn(|c| repo::connections::get_by_id(c, id))
    }

    pub fn save(&self, input: SaveConnectionInput) -> AppResult<ConnectionProfile> {
        let copy_from = input
            .copy_secret_from
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let incoming = incoming_password(&input);
        let clear = input.clear_secret;
        let input = strip_plugin_password(input);

        let mut saved = self.db.with_conn(|c| repo::connections::save(c, input))?;

        if clear {
            self.secrets.delete(&saved.id)?;
            saved.password_set = false;
        } else if let Some(pw) = incoming {
            self.secrets.set(&saved.id, &pw)?;
            let pointer = secret_account(&saved.id);
            self.db
                .with_conn(|c| repo::connections::set_secret_ref(c, &saved.id, Some(&pointer)))?;
            saved.password_set = true;
        } else if let Some(from) = copy_from {
            if self.secrets.copy(&from, &saved.id)? {
                let pointer = secret_account(&saved.id);
                self.db.with_conn(|c| {
                    repo::connections::set_secret_ref(c, &saved.id, Some(&pointer))
                })?;
                saved.password_set = true;
            }
        }

        saved.password = None;
        Ok(saved)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let id = id.trim();
        if id.is_empty() {
            return Err(AppError::Invalid("connection id is required".into()));
        }
        let _ = self.secrets.delete(id);
        self.db.with_conn(|c| repo::connections::delete(c, id))
    }

    pub fn touch_connected(&self, id: &str) -> AppResult<ConnectionProfile> {
        self.db
            .with_conn(|c| repo::connections::touch_last_connected(c, id))
    }

    /// Keychain first, then migrate leftover plaintext in SQLite.
    pub fn resolve_password(&self, id: &str) -> AppResult<Option<String>> {
        let id = id.trim();
        if id.is_empty() {
            return Err(AppError::Invalid("connection id is required".into()));
        }
        if let Some(pw) = self.secrets.get(id)? {
            return Ok(Some(pw));
        }
        self.migrate_row(id)
    }

    /// Move plaintext `secret_ref` values into the keychain. Safe to call at boot.
    pub fn migrate_plaintext_secrets(&self) -> AppResult<()> {
        let rows = self.db.with_conn(repo::connections::list_secret_refs)?;
        for (id, stored) in rows {
            let Some(raw) = stored else { continue };
            if is_placeholder_secret_ref(&raw) {
                let _ = self
                    .db
                    .with_conn(|c| repo::connections::set_secret_ref(c, &id, None));
                continue;
            }
            if is_keychain_pointer(&raw) {
                continue;
            }
            if is_legacy_plaintext_secret(&raw) {
                let _ = self.migrate_plaintext(&id, &raw);
            }
        }
        Ok(())
    }

    fn migrate_row(&self, id: &str) -> AppResult<Option<String>> {
        let stored = self
            .db
            .with_conn(|c| repo::connections::get_secret_ref(c, id))?;
        let Some(raw) = stored else {
            return Ok(None);
        };
        if is_placeholder_secret_ref(&raw) {
            self.db
                .with_conn(|c| repo::connections::set_secret_ref(c, id, None))?;
            return Ok(None);
        }
        if is_keychain_pointer(&raw) {
            return Ok(None);
        }
        if is_legacy_plaintext_secret(&raw) {
            return self.migrate_plaintext(id, &raw);
        }
        Ok(None)
    }

    fn migrate_plaintext(&self, id: &str, plaintext: &str) -> AppResult<Option<String>> {
        self.secrets.set(id, plaintext)?;
        let pointer = secret_account(id);
        self.db
            .with_conn(|c| repo::connections::set_secret_ref(c, id, Some(&pointer)))?;
        Ok(Some(plaintext.to_string()))
    }
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
        assert_eq!(raw.as_deref(), Some(secret_account(&saved.id).as_str()));
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
        assert_eq!(raw.as_deref(), Some(secret_account(&saved.id).as_str()));
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
}
