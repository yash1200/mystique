//! AI provider API keys, sealed in the vault as `ai:<provider>`.
//! The frontend never sees plaintext.

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
}

impl SecretStore {
    pub fn new(app: Arc<AppServices>) -> Self {
        Self { app }
    }

    fn kind(provider: AgentProvider) -> SecretKind<'static> {
        SecretKind::Ai(provider.as_str())
    }

    pub fn get(&self, provider: AgentProvider) -> AppResult<Option<String>> {
        Ok(self
            .app
            .vault()
            .get(Self::kind(provider))?
            .filter(|key| !key.trim().is_empty()))
    }

    pub fn set(&self, provider: AgentProvider, key: &str) -> AppResult<()> {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return self.delete(provider);
        }
        self.app.vault().set(Self::kind(provider), trimmed)
    }

    pub fn delete(&self, provider: AgentProvider) -> AppResult<()> {
        self.app.vault().delete(Self::kind(provider))?;
        Ok(())
    }

    /// Whether a key is saved. Never loads the vault key.
    pub fn status(&self) -> Vec<AiKeyStatus> {
        AgentProvider::ALL
            .into_iter()
            .map(|p| AiKeyStatus {
                provider: p.as_str().into(),
                has_key: !p.needs_key() || self.app.vault().has(Self::kind(p)).unwrap_or(false),
            })
            .collect()
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
        }
        // A new process: the Settings page lists keys without unlocking.
        keys.loads.store(0, Ordering::SeqCst);
        let store = SecretStore::new(open());
        assert!(has_key(&store, "anthropic"));
        assert!(!has_key(&store, "openai"));
        assert_eq!(keys.loads.load(Ordering::SeqCst), 0);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
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
