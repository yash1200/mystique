//! Saved secrets: AES-256-GCM envelopes in SQLite, sealed with one vault key.
//!
//! The key is the only thing Mystique keeps in the OS keychain
//! (`dev.mystique.db` / `vault.v1`). Debug builds cache it in a 0600 file; see
//! [`vault`]. It loads on the first secret use, never at startup, and is cached
//! for the process, a denied prompt included, until [`SecretStore::retry_unlock`].
//! The renderer never sees a saved secret.
//!
//! `connections.secret_ref` and `ssh_secret_ref` hold [`VAULT_SECRET_REF`] when
//! the secret is an envelope here. Older rows still hold a keychain account
//! (`conn.<install>.<id>`, `conn.<id>`, `connection.<id>.password`). The first
//! use reads that item once, moves it into the vault, then deletes it. A null
//! pointer is not a secret, so no stale `conn.<id>` is ever read for it.

mod envelope;
pub mod keychain;
mod session;
pub mod vault;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::domain::{is_legacy_keychain_pointer, is_placeholder_secret_ref, VAULT_SECRET_REF};
use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

use self::envelope::{Slot, KEY_VERSION};
use self::keychain::{KeychainKeySource, KeyringBackend, LOCKED_MESSAGE};
use self::vault::Vault;

pub use self::keychain::VAULT_ACCOUNT;
pub use self::session::PendingPassword;
pub(crate) use self::session::SessionPasswords;
pub use self::vault::{
    CachedKeySource, FileKeySource, KeyChoice, KeySource, MemoryKeySource, Prompt, VaultError,
    VaultKey, VaultStatus, KEY_FILE_NAME, KEY_SOURCE_ENV,
};

const INSTALL_ID_KEY: &str = "install_id";

const LEGACY_OFF_MESSAGE: &str = "This password is still in the system keychain from an older version, and MYSTIQUE_VAULT_KEY_SOURCE=file keeps this run away from the keychain. Run once without it to move the password, or enter it again.";

/// Keychain items from builds before the vault.
pub(crate) trait SecretBackend: Send + Sync {
    fn get(&self, account: &str, prompt: Prompt) -> AppResult<Option<String>>;
    /// Delete without UI. False when absent or when it would need a prompt.
    fn delete(&self, account: &str) -> AppResult<bool>;
    /// Account names under the service, attributes only.
    fn list(&self) -> AppResult<Vec<String>>;
}

#[derive(Default)]
struct MemoryBackend {
    inner: Mutex<HashMap<String, String>>,
}

impl SecretBackend for MemoryBackend {
    fn get(&self, account: &str, _prompt: Prompt) -> AppResult<Option<String>> {
        let map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        Ok(map.get(account).cloned())
    }

    fn delete(&self, account: &str) -> AppResult<bool> {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        Ok(map.remove(account).is_some())
    }

    fn list(&self) -> AppResult<Vec<String>> {
        let map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        Ok(map.keys().cloned().collect())
    }
}

/// Old keychain items, read at most once each per process. Misses are cached
/// too, and parallel reads of one account share a single OS read.
struct LegacyItems {
    backend: Arc<dyn SecretBackend>,
    /// `None` is a cached miss.
    cache: Mutex<HashMap<String, Option<String>>>,
    reads: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl LegacyItems {
    fn new(backend: Arc<dyn SecretBackend>) -> Self {
        Self {
            backend,
            cache: Mutex::new(HashMap::new()),
            reads: Mutex::new(HashMap::new()),
        }
    }

    fn cache_lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Option<String>>> {
        self.cache.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Errors are not cached, so a denied prompt can be retried.
    fn get(&self, account: &str, prompt: Prompt) -> AppResult<Option<String>> {
        if let Some(cached) = self.cache_lock().get(account).cloned() {
            return Ok(cached);
        }
        let gate = {
            let mut gates = self.reads.lock().unwrap_or_else(|p| p.into_inner());
            Arc::clone(gates.entry(account.to_string()).or_default())
        };
        let _hold = gate.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(cached) = self.cache_lock().get(account).cloned() {
            return Ok(cached);
        }
        let got = self.backend.get(account, prompt)?;
        self.cache_lock().insert(account.to_string(), got.clone());
        Ok(got)
    }

    fn delete(&self, account: &str) -> bool {
        let deleted = self.backend.delete(account).unwrap_or(false);
        self.cache_lock().insert(account.to_string(), None);
        deleted
    }
}

/// Who a secret belongs to. Part of the envelope AAD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretOwner<'a> {
    Connection(&'a str),
    App,
}

impl SecretOwner<'_> {
    fn kind(&self) -> &'static str {
        match self {
            Self::Connection(_) => "connection",
            Self::App => "app",
        }
    }

    fn id(&self) -> &str {
        match self {
            Self::Connection(id) => id.trim(),
            Self::App => "app",
        }
    }
}

/// `password`, `ssh`, `tls`, `ai:<provider>` or `mcp_token`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind<'a> {
    Password,
    Ssh,
    Tls,
    Ai(&'a str),
    McpToken,
}

impl SecretKind<'_> {
    pub fn name(&self) -> String {
        match self {
            Self::Password => "password".into(),
            Self::Ssh => "ssh".into(),
            Self::Tls => "tls".into(),
            Self::Ai(provider) => format!("ai:{}", provider.trim()),
            Self::McpToken => "mcp_token".into(),
        }
    }
}

type UnlockHook = Arc<dyn Fn() + Send + Sync>;

/// The vault plus read-once access to the old keychain items.
pub struct SecretStore {
    vault: Vault,
    /// `None` when the key is not in the keychain: old items are never touched.
    legacy: Option<LegacyItems>,
    on_unlock: Mutex<Option<UnlockHook>>,
    /// Passwords of connections that do not save one. Memory only.
    session: SessionPasswords,
}

impl SecretStore {
    /// The app's store. The key source follows the build; see [`vault`].
    pub fn os(data_dir: &Path) -> Self {
        let choice = KeyChoice::from_env();
        let source = choice.source(data_dir, Arc::new(KeychainKeySource));
        // Old items move into the vault in every build that may use the keychain.
        let legacy = choice
            .uses_keychain()
            .then(|| Arc::new(KeyringBackend) as Arc<dyn SecretBackend>);
        Self::from_parts(source, legacy)
    }

    /// In-process key and items. Never touches the OS keychain.
    pub fn memory() -> Self {
        Self::with_key_source(Arc::new(MemoryKeySource::new()))
    }

    /// A custom key source with in-memory old items. For tests in other crates.
    pub fn with_key_source(source: Arc<dyn KeySource>) -> Self {
        Self::from_parts(source, Some(Arc::new(MemoryBackend::default())))
    }

    pub(crate) fn from_parts(
        source: Arc<dyn KeySource>,
        legacy: Option<Arc<dyn SecretBackend>>,
    ) -> Self {
        Self {
            vault: Vault::new(source),
            legacy: legacy.map(LegacyItems::new),
            on_unlock: Mutex::new(None),
            session: SessionPasswords::default(),
        }
    }

    pub(crate) fn session(&self) -> &SessionPasswords {
        &self.session
    }

    /// Runs once, right after the key first loads in this process.
    pub(crate) fn on_first_unlock(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.on_unlock.lock().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(hook));
    }

    /// The vault key. Loads it on first use (one prompt at most, on a release build).
    pub(crate) fn unlock(&self, prompt: Prompt) -> AppResult<VaultKey> {
        let (key, fresh) = self.vault.key(prompt).map_err(|e| match e {
            VaultError::Locked => AppError::SecretStore(LOCKED_MESSAGE.into()),
            VaultError::Denied(msg) => AppError::SecretStore(format!(
                "Saved passwords could not be unlocked ({msg}). Use Settings > Retry keychain to try again."
            )),
        })?;
        if fresh {
            let hook = self
                .on_unlock
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            if let Some(hook) = hook {
                hook();
            }
        }
        Ok(key)
    }

    /// Never loads the key.
    pub fn status(&self) -> VaultStatus {
        self.vault.status()
    }

    /// Clear a cached denial and ask again.
    pub fn retry_unlock(&self) -> VaultStatus {
        self.vault.retry();
        let _ = self.unlock(Prompt::Allowed);
        self.vault.status()
    }

    pub fn key_source(&self) -> &'static str {
        self.vault.source_name()
    }

    /// Open one secret. No row, no key load.
    pub(crate) fn read(
        &self,
        db: &AppDatabase,
        owner: SecretOwner<'_>,
        kind: SecretKind<'_>,
        prompt: Prompt,
    ) -> AppResult<Option<String>> {
        let kind = kind.name();
        let found = db.with_conn(|c| {
            let Some(row) = repo::secrets::get(c, owner.kind(), owner.id(), &kind)? else {
                return Ok(None);
            };
            Ok(Some((row, install_id(c)?)))
        })?;
        let Some((row, install)) = found else {
            return Ok(None);
        };
        // The key loads outside the database lock: a prompt can take a while.
        let key = self.unlock(prompt)?;
        let slot = Slot {
            install: &install,
            owner_kind: owner.kind(),
            owner_id: owner.id(),
            kind: &kind,
        };
        envelope::open(&key, &slot, &row.envelope).map(Some)
    }

    /// Seal and upsert. Callers unlock first, outside their transaction.
    pub(crate) fn write(
        &self,
        conn: &Connection,
        key: &VaultKey,
        owner: SecretOwner<'_>,
        kind: SecretKind<'_>,
        plaintext: &str,
    ) -> AppResult<()> {
        let kind = kind.name();
        let install = install_id(conn)?;
        let slot = Slot {
            install: &install,
            owner_kind: owner.kind(),
            owner_id: owner.id(),
            kind: &kind,
        };
        let sealed = envelope::seal(key, &slot, plaintext)?;
        repo::secrets::upsert(conn, owner.kind(), owner.id(), &kind, &sealed, KEY_VERSION)
    }

    pub(crate) fn remove(
        conn: &Connection,
        owner: SecretOwner<'_>,
        kind: SecretKind<'_>,
    ) -> AppResult<()> {
        repo::secrets::delete(conn, owner.kind(), owner.id(), &kind.name())?;
        Ok(())
    }

    pub(crate) fn remove_owner(conn: &Connection, owner: SecretOwner<'_>) -> AppResult<()> {
        repo::secrets::delete_owner(conn, owner.kind(), owner.id())?;
        Ok(())
    }

    pub(crate) fn has(
        conn: &Connection,
        owner: SecretOwner<'_>,
        kind: SecretKind<'_>,
    ) -> AppResult<bool> {
        repo::secrets::exists(conn, owner.kind(), owner.id(), &kind.name())
    }

    pub(crate) fn legacy_enabled(&self) -> bool {
        self.legacy.is_some()
    }

    /// One old keychain item, read at most once per process.
    pub(crate) fn legacy_get(&self, account: &str, prompt: Prompt) -> AppResult<Option<String>> {
        let account = account.trim();
        if account.is_empty() {
            return Ok(None);
        }
        match &self.legacy {
            Some(items) => items.get(account, prompt),
            None => Err(AppError::SecretStore(LEGACY_OFF_MESSAGE.into())),
        }
    }

    /// Best effort and never shows UI. False when skipped.
    pub(crate) fn legacy_delete(&self, account: &str) -> bool {
        let account = account.trim();
        match &self.legacy {
            Some(items) if !account.is_empty() => items.delete(account),
            _ => false,
        }
    }

    pub(crate) fn legacy_list(&self) -> AppResult<Vec<String>> {
        match &self.legacy {
            Some(items) => items.backend.list(),
            None => Ok(Vec::new()),
        }
    }

    /// Drop the cached key and item reads. Tests use it to prove a path reads nothing.
    #[cfg(test)]
    pub(crate) fn forget_cached(&self) {
        self.vault.forget();
        if let Some(items) = &self.legacy {
            items.cache_lock().clear();
        }
    }
}

/// The UUID in `app_settings.install_id`, created on first need. Part of every
/// envelope's AAD and of the old `conn.<install>.<id>` account names.
pub(crate) fn install_id(conn: &Connection) -> AppResult<String> {
    if let Some(row) = repo::settings::get(conn, INSTALL_ID_KEY)? {
        if uuid::Uuid::parse_str(row.value.trim()).is_ok() {
            return Ok(row.value.trim().to_string());
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    repo::settings::set(conn, INSTALL_ID_KEY, &id)?;
    Ok(id)
}

/// True when `secret_ref` is leftover plaintext, not a pointer or placeholder.
pub fn is_legacy_plaintext_secret(secret_ref: &str) -> bool {
    let t = secret_ref.trim();
    !t.is_empty()
        && t != VAULT_SECRET_REF
        && !is_placeholder_secret_ref(t)
        && !t.starts_with("conn.")
        && !is_legacy_keychain_pointer(t)
}

/// Fake keychain that counts reads per account, and a counting key source.
#[cfg(test)]
pub(crate) mod counting {
    use super::*;

    pub(crate) use super::vault::counting::CountingKeySource;

    #[derive(Default)]
    pub(crate) struct CountingBackend {
        map: Mutex<HashMap<String, String>>,
        reads: Mutex<HashMap<String, usize>>,
    }

    impl CountingBackend {
        /// An item an older build left behind.
        pub fn plant(&self, account: &str, secret: &str) {
            let mut map = self.map.lock().unwrap_or_else(|p| p.into_inner());
            map.insert(account.to_string(), secret.to_string());
        }

        pub fn reads_of(&self, account: &str) -> usize {
            let reads = self.reads.lock().unwrap_or_else(|p| p.into_inner());
            reads.get(account).copied().unwrap_or(0)
        }

        pub fn total_reads(&self) -> usize {
            let reads = self.reads.lock().unwrap_or_else(|p| p.into_inner());
            reads.values().sum()
        }

        pub fn read_accounts(&self) -> Vec<String> {
            let reads = self.reads.lock().unwrap_or_else(|p| p.into_inner());
            let mut out: Vec<String> = reads.keys().cloned().collect();
            out.sort();
            out
        }

        pub fn reset_reads(&self) {
            self.reads.lock().unwrap_or_else(|p| p.into_inner()).clear();
        }

        pub fn contains(&self, account: &str) -> bool {
            let map = self.map.lock().unwrap_or_else(|p| p.into_inner());
            map.contains_key(account)
        }
    }

    impl SecretBackend for CountingBackend {
        fn get(&self, account: &str, _prompt: Prompt) -> AppResult<Option<String>> {
            *self
                .reads
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .entry(account.to_string())
                .or_default() += 1;
            let map = self.map.lock().unwrap_or_else(|p| p.into_inner());
            Ok(map.get(account).cloned())
        }

        fn delete(&self, account: &str) -> AppResult<bool> {
            let mut map = self.map.lock().unwrap_or_else(|p| p.into_inner());
            Ok(map.remove(account).is_some())
        }

        fn list(&self) -> AppResult<Vec<String>> {
            let map = self.map.lock().unwrap_or_else(|p| p.into_inner());
            let mut out: Vec<String> = map.keys().cloned().collect();
            out.sort();
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::counting::{CountingBackend, CountingKeySource};
    use super::*;

    fn store(keys: &Arc<CountingKeySource>, items: &Arc<CountingBackend>) -> SecretStore {
        SecretStore::from_parts(keys.clone(), Some(items.clone()))
    }

    #[test]
    fn a_missing_old_item_is_read_once() {
        let keys = Arc::new(CountingKeySource::default());
        let items = Arc::new(CountingBackend::default());
        let store = store(&keys, &items);
        assert!(store
            .legacy_get("conn.x.tls", Prompt::Allowed)
            .unwrap()
            .is_none());
        assert!(store
            .legacy_get("conn.x.tls", Prompt::Allowed)
            .unwrap()
            .is_none());
        assert_eq!(items.reads_of("conn.x.tls"), 1);
    }

    #[test]
    fn parallel_reads_of_one_old_item_make_one_os_read() {
        let keys = Arc::new(CountingKeySource::default());
        let items = Arc::new(CountingBackend::default());
        items.plant("conn.a", "pw");
        let store = Arc::new(store(&keys, &items));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || store.legacy_get("conn.a", Prompt::Allowed).unwrap())
            })
            .collect();
        for h in handles {
            assert_eq!(h.join().unwrap().as_deref(), Some("pw"));
        }
        assert_eq!(items.reads_of("conn.a"), 1);
    }

    #[test]
    fn a_missing_row_loads_no_key() {
        let keys = Arc::new(CountingKeySource::default());
        let items = Arc::new(CountingBackend::default());
        let store = store(&keys, &items);
        let db = AppDatabase::open_in_memory().unwrap();
        let got = store
            .read(&db, SecretOwner::App, SecretKind::McpToken, Prompt::Never)
            .unwrap();
        assert!(got.is_none());
        assert_eq!(keys.loads(), 0);
    }

    #[test]
    fn write_then_read_stores_the_key_version() {
        let keys = Arc::new(CountingKeySource::default());
        let items = Arc::new(CountingBackend::default());
        let store = store(&keys, &items);
        let db = AppDatabase::open_in_memory().unwrap();
        let key = store.unlock(Prompt::Allowed).unwrap();
        db.with_tx(|c| store.write(c, &key, SecretOwner::App, SecretKind::Ai("openai"), "sk-1"))
            .unwrap();
        let row = db
            .with_conn(|c| repo::secrets::get(c, "app", "app", "ai:openai"))
            .unwrap()
            .unwrap();
        assert_eq!(row.key_version, KEY_VERSION);
        assert!(row.envelope.starts_with("v1."));
        assert!(!row.envelope.contains("sk-1"));
        let got = store
            .read(
                &db,
                SecretOwner::App,
                SecretKind::Ai("openai"),
                Prompt::Never,
            )
            .unwrap();
        assert_eq!(got.as_deref(), Some("sk-1"));
        // A row copied to another kind does not open.
        db.with_conn(|c| {
            repo::secrets::upsert(c, "app", "app", "ai:google", &row.envelope, KEY_VERSION)
        })
        .unwrap();
        assert!(store
            .read(
                &db,
                SecretOwner::App,
                SecretKind::Ai("google"),
                Prompt::Never
            )
            .is_err());
        assert_eq!(keys.loads(), 1);
    }

    #[test]
    fn a_store_without_old_items_never_reads_the_keychain() {
        let store = SecretStore::from_parts(Arc::new(MemoryKeySource::new()), None);
        assert!(!store.legacy_enabled());
        let err = store.legacy_get("conn.1", Prompt::Allowed).unwrap_err();
        assert!(err.to_string().contains("MYSTIQUE_VAULT_KEY_SOURCE"));
        assert!(!store.legacy_delete("conn.1"));
        assert!(store.legacy_list().unwrap().is_empty());
    }

    #[test]
    fn legacy_plaintext_detection() {
        assert!(is_legacy_plaintext_secret("s3cret"));
        assert!(!is_legacy_plaintext_secret("vault"));
        assert!(!is_legacy_plaintext_secret("conn.12"));
        assert!(!is_legacy_plaintext_secret("mystique/conn/12"));
        assert!(!is_legacy_plaintext_secret("connection.abc.password"));
        assert!(!is_legacy_plaintext_secret(""));
    }
}
