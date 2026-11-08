//! The one vault key: loaded on first use, then cached for the process.
//!
//! Key sources:
//! - keychain: one item, service `dev.mystique.db`, account `vault.v1`.
//!   Default for release builds.
//! - file: `<app data dir>/vault.key`, mode 0600. Default for debug builds, so
//!   `pnpm tauri dev` never touches the keychain.
//! - memory: tests.
//!
//! `MYSTIQUE_VAULT_KEY_SOURCE=keychain|file` overrides the default in any
//! build. That is the only way a release build uses the file: it is meant for
//! machines without a usable keychain, and the key then sits on disk next to
//! the database it protects.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use serde::Serialize;

pub const KEY_SOURCE_ENV: &str = "MYSTIQUE_VAULT_KEY_SOURCE";
pub const KEY_FILE_NAME: &str = "vault.key";

/// 32 random bytes. Debug output never shows them.
#[derive(Clone, PartialEq, Eq)]
pub struct VaultKey([u8; 32]);

impl VaultKey {
    pub fn random() -> Self {
        let mut key = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut key);
        Self(key)
    }

    pub(crate) fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub(crate) fn encode(&self) -> String {
        B64.encode(self.0)
    }

    pub(crate) fn decode(text: &str) -> Option<Self> {
        let raw = B64.decode(text.trim()).ok()?;
        <[u8; 32]>::try_from(raw.as_slice()).ok().map(Self)
    }
}

impl std::fmt::Debug for VaultKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VaultKey(..)")
    }
}

/// Whether a load may show UI (a keychain prompt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    /// The app and in-app agent: the user is there to answer.
    Allowed,
    /// MCP requests: fail instead of prompting.
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    /// The source refused: a denied prompt, an unreadable file. Cached until `retry`.
    Denied(String),
    /// A `Prompt::Never` load needed a prompt. Not cached.
    Locked,
}

pub trait KeySource: Send + Sync {
    /// Read the key, creating it on first use. With `Prompt::Never` this must
    /// not show UI; return `VaultError::Locked` instead.
    fn load(&self, prompt: Prompt) -> Result<VaultKey, VaultError>;

    /// `keychain`, `file` or `memory`.
    fn name(&self) -> &'static str;
}

impl<T: KeySource + ?Sized> KeySource for Arc<T> {
    fn load(&self, prompt: Prompt) -> Result<VaultKey, VaultError> {
        (**self).load(prompt)
    }

    fn name(&self) -> &'static str {
        (**self).name()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    /// `keychain`, `file` or `memory`.
    pub source: String,
    /// `idle` (not loaded yet), `unlocked` or `denied`.
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Holds the key for the process. One per `SecretStore`, which `AppServices`
/// owns once per process.
pub(crate) struct Vault {
    source: Arc<dyn KeySource>,
    /// Held while loading, so parallel first uses make one load.
    state: Mutex<Option<Result<VaultKey, VaultError>>>,
}

impl Vault {
    pub fn new(source: Arc<dyn KeySource>) -> Self {
        Self {
            source,
            state: Mutex::new(None),
        }
    }

    /// The key, and whether this call loaded it.
    pub fn key(&self, prompt: Prompt) -> Result<(VaultKey, bool), VaultError> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(done) = state.as_ref() {
            return done.clone().map(|key| (key, false));
        }
        match self.source.load(prompt) {
            Err(VaultError::Locked) => Err(VaultError::Locked),
            done => {
                *state = Some(done.clone());
                done.map(|key| (key, true))
            }
        }
    }

    /// Forget a cached denial so the next use asks again. A loaded key stays.
    pub fn retry(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if matches!(*state, Some(Err(_))) {
            *state = None;
        }
    }

    pub fn status(&self) -> VaultStatus {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let (state, message) = match state.as_ref() {
            None => ("idle", None),
            Some(Ok(_)) => ("unlocked", None),
            Some(Err(VaultError::Denied(msg))) => ("denied", Some(msg.clone())),
            Some(Err(VaultError::Locked)) => ("idle", None),
        };
        VaultStatus {
            source: self.source.name().into(),
            state: state.into(),
            message,
        }
    }

    pub fn source_name(&self) -> &'static str {
        self.source.name()
    }

    /// Drop the cached key. Tests use it to prove a path loads nothing.
    #[cfg(test)]
    pub fn forget(&self) {
        *self.state.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

/// The source for this build: see the module docs.
pub fn default_key_source(data_dir: &Path) -> Arc<dyn KeySource> {
    let file = || Arc::new(FileKeySource::new(data_dir.join(KEY_FILE_NAME))) as Arc<dyn KeySource>;
    let keychain = || Arc::new(super::keychain::KeychainKeySource) as Arc<dyn KeySource>;
    match std::env::var(KEY_SOURCE_ENV).ok().as_deref().map(str::trim) {
        Some("keychain") => keychain(),
        Some("file") => file(),
        _ if cfg!(debug_assertions) => file(),
        _ => keychain(),
    }
}

/// `<app data dir>/vault.key`, base64 of 32 bytes, mode 0600.
pub struct FileKeySource {
    path: PathBuf,
}

impl FileKeySource {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl KeySource for FileKeySource {
    fn load(&self, _prompt: Prompt) -> Result<VaultKey, VaultError> {
        read_or_create_key_file(&self.path)
            .map_err(|e| VaultError::Denied(format!("vault key file {}: {e}", self.path.display())))
    }

    fn name(&self) -> &'static str {
        "file"
    }
}

fn read_or_create_key_file(path: &Path) -> std::io::Result<VaultKey> {
    match read_key_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        other => return other,
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let key = VaultKey::random();
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(key.encode().as_bytes())?;
            file.sync_all()?;
            Ok(key)
        }
        // Another process created it first and may still be writing.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            for _ in 0..20 {
                if let Ok(key) = read_key_file(path) {
                    return Ok(key);
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            read_key_file(path)
        }
        Err(e) => Err(e),
    }
}

fn read_key_file(path: &Path) -> std::io::Result<VaultKey> {
    let text = std::fs::read_to_string(path)?;
    let key = VaultKey::decode(&text).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "not a 32-byte base64 key")
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)?.permissions().mode();
        if mode & 0o077 != 0 {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    Ok(key)
}

/// A random key that lives as long as the process. `SecretStore::memory()`.
pub struct MemoryKeySource {
    key: VaultKey,
}

impl MemoryKeySource {
    pub fn new() -> Self {
        Self {
            key: VaultKey::random(),
        }
    }
}

impl Default for MemoryKeySource {
    fn default() -> Self {
        Self::new()
    }
}

impl KeySource for MemoryKeySource {
    fn load(&self, _prompt: Prompt) -> Result<VaultKey, VaultError> {
        Ok(self.key.clone())
    }

    fn name(&self) -> &'static str {
        "memory"
    }
}

/// Key source that counts loads, for tests.
#[cfg(test)]
pub(crate) mod counting {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    #[derive(Default)]
    pub(crate) struct CountingKeySource {
        key: Mutex<Option<VaultKey>>,
        loads: AtomicUsize,
        /// Every load is refused, like a denied prompt.
        pub deny: AtomicBool,
        /// Loads without UI fail, like an item whose ACL needs a prompt.
        pub needs_ui: AtomicBool,
        /// Holds each load open so parallel callers overlap.
        pub delay: Option<Duration>,
    }

    impl CountingKeySource {
        pub fn slow(delay: Duration) -> Self {
            Self {
                delay: Some(delay),
                ..Self::default()
            }
        }

        pub fn loads(&self) -> usize {
            self.loads.load(Ordering::SeqCst)
        }

        pub fn reset(&self) {
            self.loads.store(0, Ordering::SeqCst);
        }

        /// Hand out this key, as a source that already holds one.
        pub fn seed(&self, key: VaultKey) {
            *self.key.lock().unwrap_or_else(|p| p.into_inner()) = Some(key);
        }
    }

    impl KeySource for CountingKeySource {
        fn load(&self, prompt: Prompt) -> Result<VaultKey, VaultError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            if let Some(delay) = self.delay {
                std::thread::sleep(delay);
            }
            if self.deny.load(Ordering::SeqCst) {
                return Err(VaultError::Denied("denied".into()));
            }
            if prompt == Prompt::Never && self.needs_ui.load(Ordering::SeqCst) {
                return Err(VaultError::Locked);
            }
            let mut key = self.key.lock().unwrap_or_else(|p| p.into_inner());
            Ok(key.get_or_insert_with(VaultKey::random).clone())
        }

        fn name(&self) -> &'static str {
            "memory"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::counting::CountingKeySource;
    use super::*;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    #[test]
    fn nothing_loads_until_first_use() {
        let source = Arc::new(CountingKeySource::default());
        let vault = Vault::new(source.clone());
        assert_eq!(vault.status().state, "idle");
        assert_eq!(source.loads(), 0);
        let (first, loaded) = vault.key(Prompt::Allowed).unwrap();
        assert!(loaded);
        let (again, loaded) = vault.key(Prompt::Never).unwrap();
        assert!(!loaded);
        assert_eq!(first, again);
        assert_eq!(source.loads(), 1);
        assert_eq!(vault.status().state, "unlocked");
    }

    #[test]
    fn parallel_first_use_loads_once() {
        let source = Arc::new(CountingKeySource::slow(Duration::from_millis(50)));
        let vault = Arc::new(Vault::new(source.clone()));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let vault = Arc::clone(&vault);
                std::thread::spawn(move || vault.key(Prompt::Allowed).unwrap().0)
            })
            .collect();
        let keys: Vec<VaultKey> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(keys.windows(2).all(|w| w[0] == w[1]));
        assert_eq!(source.loads(), 1);
    }

    #[test]
    fn a_denial_is_cached_until_retry() {
        let source = Arc::new(CountingKeySource::default());
        source.deny.store(true, Ordering::SeqCst);
        let vault = Vault::new(source.clone());
        assert!(matches!(
            vault.key(Prompt::Allowed),
            Err(VaultError::Denied(_))
        ));
        assert!(matches!(
            vault.key(Prompt::Allowed),
            Err(VaultError::Denied(_))
        ));
        assert_eq!(source.loads(), 1, "a denied prompt is not shown again");
        assert_eq!(vault.status().state, "denied");

        source.deny.store(false, Ordering::SeqCst);
        vault.retry();
        assert_eq!(vault.status().state, "idle");
        assert!(vault.key(Prompt::Allowed).is_ok());
        assert_eq!(source.loads(), 2);
        // Retry keeps a loaded key.
        vault.retry();
        assert!(vault.key(Prompt::Allowed).is_ok());
        assert_eq!(source.loads(), 2);
    }

    #[test]
    fn a_locked_no_ui_load_is_not_cached() {
        let source = Arc::new(CountingKeySource::default());
        source.needs_ui.store(true, Ordering::SeqCst);
        let vault = Vault::new(source.clone());
        assert_eq!(vault.key(Prompt::Never).unwrap_err(), VaultError::Locked);
        assert_eq!(vault.status().state, "idle");
        // The app can still prompt, and then MCP uses the cached key.
        assert!(vault.key(Prompt::Allowed).is_ok());
        assert!(vault.key(Prompt::Never).is_ok());
        assert_eq!(source.loads(), 2);
    }

    #[test]
    fn the_file_source_creates_a_0600_file_and_reuses_it() {
        let dir = std::env::temp_dir().join(format!("mystique-vault-{}", uuid::Uuid::new_v4()));
        let path = dir.join(KEY_FILE_NAME);
        let source = FileKeySource::new(&path);
        let first = source.load(Prompt::Never).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert_eq!(
            VaultKey::decode(&std::fs::read_to_string(&path).unwrap()),
            Some(first.clone())
        );
        let again = FileKeySource::new(&path).load(Prompt::Allowed).unwrap();
        assert_eq!(first, again);

        std::fs::write(&path, "not a key").unwrap();
        assert!(matches!(
            FileKeySource::new(&path).load(Prompt::Allowed),
            Err(VaultError::Denied(_))
        ));
        let _ = std::fs::remove_dir_all(dir);
    }
}
