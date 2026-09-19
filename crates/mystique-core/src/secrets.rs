//! OS keychain for connection passwords, with an in-process cache.
//!
//! SQLite stores only an opaque pointer (`conn.<id>`). The renderer never
//! sees a saved secret. Cache so macOS does not prompt on every read.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::domain::{is_placeholder_secret_ref, secret_account};
use crate::error::{AppError, AppResult};

const SERVICE: &str = "dev.mystique.db";

pub trait SecretBackend: Send + Sync {
    fn get(&self, account: &str) -> AppResult<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> AppResult<()>;
    fn delete(&self, account: &str) -> AppResult<()>;
}

struct MemoryBackend {
    inner: Mutex<HashMap<String, String>>,
}

impl MemoryBackend {
    fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl SecretBackend for MemoryBackend {
    fn get(&self, account: &str) -> AppResult<Option<String>> {
        let map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        Ok(map.get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> AppResult<()> {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.insert(account.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, account: &str) -> AppResult<()> {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.remove(account);
        Ok(())
    }
}

struct KeyringBackend;

impl SecretBackend for KeyringBackend {
    fn get(&self, account: &str) -> AppResult<Option<String>> {
        let entry = keyring_entry(account)?;
        match entry.get_password() {
            Ok(p) if !p.is_empty() => Ok(Some(p)),
            Ok(_) => Ok(None),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(secret_store_err(e)),
        }
    }

    fn set(&self, account: &str, secret: &str) -> AppResult<()> {
        keyring_entry(account)?
            .set_password(secret)
            .map_err(secret_store_err)
    }

    fn delete(&self, account: &str) -> AppResult<()> {
        match keyring_entry(account)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(secret_store_err(e)),
        }
    }
}

fn keyring_entry(account: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(SERVICE, account).map_err(secret_store_err)
}

fn secret_store_err(err: impl std::fmt::Display) -> AppError {
    AppError::SecretStore(format!(
        "OS secret store unavailable ({err}). Mystique will not store passwords in the app database."
    ))
}

/// Connection-password store. Pointer in SQLite; bytes in the OS keychain.
pub struct SecretStore {
    backend: Arc<dyn SecretBackend>,
    cache: Mutex<HashMap<String, String>>,
}

impl SecretStore {
    pub fn os() -> Self {
        Self::with_backend(Arc::new(KeyringBackend))
    }

    pub fn memory() -> Self {
        Self::with_backend(Arc::new(MemoryBackend::new()))
    }

    fn with_backend(backend: Arc<dyn SecretBackend>) -> Self {
        Self {
            backend,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn cache_lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.cache.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn get(&self, connection_id: &str) -> AppResult<Option<String>> {
        let account = secret_account(connection_id);
        if let Some(hit) = self.cache_lock().get(&account).cloned() {
            return Ok(Some(hit));
        }
        let got = self.backend.get(&account)?;
        if let Some(ref secret) = got {
            self.cache_lock().insert(account, secret.clone());
        }
        Ok(got)
    }

    pub fn set(&self, connection_id: &str, password: &str) -> AppResult<()> {
        let account = secret_account(connection_id);
        self.backend.set(&account, password)?;
        self.cache_lock().insert(account, password.to_string());
        Ok(())
    }

    pub fn delete(&self, connection_id: &str) -> AppResult<()> {
        let account = secret_account(connection_id);
        self.backend.delete(&account)?;
        self.cache_lock().remove(&account);
        Ok(())
    }

    /// Copy a stored secret to a new connection id. Returns whether a secret existed.
    pub fn copy(&self, from_id: &str, to_id: &str) -> AppResult<bool> {
        match self.get(from_id)? {
            Some(secret) => {
                self.set(to_id, &secret)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

/// True when `secret_ref` is leftover plaintext, not a keychain pointer or placeholder.
pub fn is_legacy_plaintext_secret(secret_ref: &str) -> bool {
    let t = secret_ref.trim();
    !t.is_empty() && !is_placeholder_secret_ref(t) && !t.starts_with("conn.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_roundtrip_and_cache() {
        let store = SecretStore::memory();
        assert!(store.get("1").unwrap().is_none());
        store.set("1", "s3cret").unwrap();
        assert_eq!(store.get("1").unwrap().as_deref(), Some("s3cret"));
        store.delete("1").unwrap();
        assert!(store.get("1").unwrap().is_none());
    }

    #[test]
    fn copy_secret_between_ids() {
        let store = SecretStore::memory();
        store.set("1", "pw").unwrap();
        assert!(store.copy("1", "2").unwrap());
        assert_eq!(store.get("2").unwrap().as_deref(), Some("pw"));
        assert!(!store.copy("missing", "3").unwrap());
    }

    #[test]
    fn legacy_plaintext_detection() {
        assert!(is_legacy_plaintext_secret("s3cret"));
        assert!(!is_legacy_plaintext_secret("conn.12"));
        assert!(!is_legacy_plaintext_secret("mystique/conn/12"));
        assert!(!is_legacy_plaintext_secret(""));
    }
}
