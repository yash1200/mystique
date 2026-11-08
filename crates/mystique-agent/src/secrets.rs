//! AI provider API keys, sealed in the vault as `ai:<provider>`.
//!
//! Older builds wrapped keys with a constant compiled into the binary (the
//! `ai_secrets` table) or kept them in a JSON file. The first read moves a key
//! into the vault and deletes the old copy. The frontend never sees plaintext.

pub(crate) mod crypto;
mod legacy;

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use mystique_core::{AppResult, AppServices, SecretKind};

use crate::providers::AgentProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AiKeyStatus {
    pub provider: String,
    pub has_key: bool,
}

/// Settings > AI, as the `get_ai_settings` family of commands returns it.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsView {
    pub enabled: bool,
    pub ollama_base_url: String,
    /// True after the user saves an Ollama URL (even the default).
    pub ollama_configured: bool,
    pub keys: Vec<AiKeyStatus>,
    pub providers: Vec<crate::AgentProviderInfo>,
}

#[derive(Clone)]
pub struct SecretStore {
    app: Arc<AppServices>,
    legacy_file: Option<PathBuf>,
}

impl SecretStore {
    pub fn new(app: Arc<AppServices>) -> Self {
        Self {
            app,
            legacy_file: None,
        }
    }

    pub fn with_legacy_file(app: Arc<AppServices>, path: impl Into<PathBuf>) -> Self {
        Self {
            app,
            legacy_file: Some(path.into()),
        }
    }

    /// The provider id plus older aliases (`gemini`, `grok`).
    fn ids(provider: AgentProvider) -> Vec<String> {
        let mut ids = vec![provider.as_str().to_string()];
        if matches!(provider, AgentProvider::Google) {
            ids.push("gemini".into());
        }
        if matches!(provider, AgentProvider::Xai) {
            ids.push("grok".into());
        }
        ids
    }

    fn kind(provider: AgentProvider) -> SecretKind<'static> {
        SecretKind::Ai(provider.as_str())
    }

    pub fn get(&self, provider: AgentProvider) -> AppResult<Option<String>> {
        if let Some(key) = self.app.vault().get(Self::kind(provider))? {
            if !key.trim().is_empty() {
                return Ok(Some(key));
            }
        }
        if let Some(plain) = self.take_wrapped(provider)? {
            return Ok(Some(plain));
        }
        if let Some(plain) = self.take_legacy(provider) {
            self.set(provider, &plain)?;
            return Ok(Some(plain));
        }
        Ok(None)
    }

    pub fn set(&self, provider: AgentProvider, key: &str) -> AppResult<()> {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return self.delete(provider);
        }
        self.app.vault().set(Self::kind(provider), trimmed)?;
        self.drop_wrapped(provider);
        Ok(())
    }

    pub fn delete(&self, provider: AgentProvider) -> AppResult<()> {
        self.app.vault().delete(Self::kind(provider))?;
        self.drop_wrapped(provider);
        self.drop_legacy(provider);
        Ok(())
    }

    /// Whether a key is saved anywhere. Never loads the vault key.
    pub fn status(&self) -> Vec<AiKeyStatus> {
        AgentProvider::ALL
            .into_iter()
            .map(|p| AiKeyStatus {
                provider: p.as_str().into(),
                has_key: !p.needs_key() || self.saved(p),
            })
            .collect()
    }

    fn saved(&self, provider: AgentProvider) -> bool {
        if self.app.vault().has(Self::kind(provider)).unwrap_or(false) {
            return true;
        }
        let ids = Self::ids(provider);
        if ids
            .iter()
            .any(|id| self.app.ai_secrets().get(id).ok().flatten().is_some())
        {
            return true;
        }
        self.legacy_file.as_ref().is_some_and(|path| {
            let map = legacy::read(path);
            ids.iter()
                .any(|id| map.get(id).is_some_and(|v| !v.trim().is_empty()))
        })
    }

    /// One-way: unwrap a pepper blob, seal it in the vault, drop the row.
    fn take_wrapped(&self, provider: AgentProvider) -> AppResult<Option<String>> {
        for id in Self::ids(provider) {
            let Some(row) = self.app.ai_secrets().get(&id)? else {
                continue;
            };
            let Ok(plain) = crypto::unwrap_legacy(&row.ciphertext) else {
                continue;
            };
            if plain.trim().is_empty() {
                continue;
            }
            self.set(provider, &plain)?;
            return Ok(Some(plain));
        }
        Ok(None)
    }

    fn drop_wrapped(&self, provider: AgentProvider) {
        for id in Self::ids(provider) {
            let _ = self.app.ai_secrets().delete(&id);
        }
    }

    fn take_legacy(&self, provider: AgentProvider) -> Option<String> {
        let path = self.legacy_file.as_ref()?;
        let mut map = legacy::read(path);
        let mut found = None;
        for id in Self::ids(provider) {
            if let Some(value) = map.remove(&id) {
                if !value.trim().is_empty() && found.is_none() {
                    found = Some(value);
                }
            }
        }
        if found.is_some() {
            let _ = legacy::write(path, &map);
        }
        found
    }

    fn drop_legacy(&self, provider: AgentProvider) {
        let Some(path) = self.legacy_file.as_ref() else {
            return;
        };
        let mut map = legacy::read(path);
        for id in Self::ids(provider) {
            map.remove(&id);
        }
        let _ = legacy::write(path, &map);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use mystique_core::{KeySource, Prompt, VaultError, VaultKey};

    /// Counts key loads, so a test can prove a path loads none.
    struct Keys {
        key: VaultKey,
        loads: AtomicUsize,
    }

    impl KeySource for Keys {
        fn load(&self, _prompt: Prompt) -> Result<VaultKey, VaultError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(self.key.clone())
        }

        fn name(&self) -> &'static str {
            "memory"
        }
    }

    fn harness() -> (Arc<AppServices>, Arc<Keys>) {
        let keys = Arc::new(Keys {
            key: VaultKey::random(),
            loads: AtomicUsize::new(0),
        });
        let app = Arc::new(AppServices::with_secrets(
            mystique_core::AppDatabase::open_in_memory().unwrap(),
            mystique_core::ConnectionManager::bootstrap(),
            mystique_core::SecretStore::with_key_source(keys.clone()),
        ));
        (app, keys)
    }

    fn has_key(store: &SecretStore, provider: &str) -> bool {
        store
            .status()
            .iter()
            .find(|s| s.provider == provider)
            .unwrap()
            .has_key
    }

    #[test]
    fn vault_roundtrip_and_status() {
        let (app, keys) = harness();
        let store = SecretStore::new(app.clone());
        assert!(!has_key(&store, "openai"));
        store.set(AgentProvider::Openai, " sk-live ").unwrap();
        assert_eq!(
            store.get(AgentProvider::Openai).unwrap().as_deref(),
            Some("sk-live")
        );
        assert!(app.ai_secrets().get("openai").unwrap().is_none());
        assert!(app.vault().has(SecretKind::Ai("openai")).unwrap());
        store.delete(AgentProvider::Openai).unwrap();
        assert!(store.get(AgentProvider::Openai).unwrap().is_none());
        assert!(!has_key(&store, "openai"));
        assert_eq!(keys.loads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn status_loads_no_key() {
        let path = temp_path("status", "sqlite");
        let (_, keys) = harness();
        let open = || {
            Arc::new(AppServices::with_secrets(
                mystique_core::AppDatabase::open(&path).unwrap(),
                mystique_core::ConnectionManager::bootstrap(),
                mystique_core::SecretStore::with_key_source(keys.clone()),
            ))
        };
        {
            let app = open();
            SecretStore::new(app.clone())
                .set(AgentProvider::Anthropic, "sk-ant")
                .unwrap();
            app.ai_secrets()
                .set("gemini", &crypto::wrap_legacy("g-key").unwrap())
                .unwrap();
        }
        // A new process: the Settings page lists keys without unlocking.
        keys.loads.store(0, Ordering::SeqCst);
        let store = SecretStore::new(open());
        assert!(has_key(&store, "anthropic"));
        assert!(has_key(&store, "google"), "an old row still counts");
        assert!(!has_key(&store, "openai"));
        assert_eq!(keys.loads.load(Ordering::SeqCst), 0);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[test]
    fn a_pepper_blob_moves_into_the_vault() {
        let (app, _) = harness();
        let blob = crypto::wrap_legacy("g-key").unwrap();
        app.ai_secrets().set("gemini", &blob).unwrap();
        let store = SecretStore::new(app.clone());
        assert_eq!(
            store.get(AgentProvider::Google).unwrap().as_deref(),
            Some("g-key")
        );
        assert!(app.ai_secrets().get("gemini").unwrap().is_none());
        assert!(app.ai_secrets().get("google").unwrap().is_none());
        assert_eq!(
            app.vault()
                .get(SecretKind::Ai("google"))
                .unwrap()
                .as_deref(),
            Some("g-key")
        );
        // Later reads come from the vault only.
        assert_eq!(
            store.get(AgentProvider::Google).unwrap().as_deref(),
            Some("g-key")
        );
    }

    #[test]
    fn a_legacy_file_key_moves_into_the_vault() {
        let (app, _) = harness();
        let path = temp_path("ai-keys", "json");
        legacy::write(
            &path,
            &[("openai".to_string(), "sk-file".to_string())]
                .into_iter()
                .collect(),
        )
        .unwrap();
        let store = SecretStore::with_legacy_file(app.clone(), &path);
        assert!(has_key(&store, "openai"));
        assert_eq!(
            store.get(AgentProvider::Openai).unwrap().as_deref(),
            Some("sk-file")
        );
        assert!(!path.exists(), "emptied file is removed");
        assert!(app.vault().has(SecretKind::Ai("openai")).unwrap());
    }

    fn temp_path(tag: &str, ext: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "mystique-{tag}-{}-{nanos}.{ext}",
            std::process::id()
        ))
    }
}
