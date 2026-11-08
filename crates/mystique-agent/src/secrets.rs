//! Encrypted API keys in the local SQLite `ai_secrets` table.
//!
//! No OS keychain — macOS Keychain Access prompts on every read. Keys are
//! AES-256-GCM blobs. The frontend never sees plaintext.

pub(crate) mod crypto;
mod legacy;

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use mystique_core::{AppResult, AppServices};

use crate::providers::AgentProvider;

use self::crypto::{decrypt, encrypt};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiKeyStatus {
    pub provider: String,
    pub has_key: bool,
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

    pub fn get(&self, provider: AgentProvider) -> AppResult<Option<String>> {
        for id in Self::ids(provider) {
            if let Some(row) = self.app.ai_secrets().get(&id)? {
                if let Ok(plain) = decrypt(&row.ciphertext) {
                    if !plain.trim().is_empty() {
                        if id != provider.as_str() {
                            let _ = self.set(provider, &plain);
                        }
                        return Ok(Some(plain));
                    }
                }
            }
        }
        if let Some(plain) = self.take_legacy(provider) {
            let _ = self.set(provider, &plain);
            return Ok(Some(plain));
        }
        Ok(None)
    }

    pub fn set(&self, provider: AgentProvider, key: &str) -> AppResult<()> {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return self.delete(provider);
        }
        let blob = encrypt(trimmed)?;
        self.app.ai_secrets().set(provider.as_str(), &blob)?;
        if matches!(provider, AgentProvider::Google) {
            let _ = self.app.ai_secrets().delete("gemini");
        }
        if matches!(provider, AgentProvider::Xai) {
            let _ = self.app.ai_secrets().delete("grok");
        }
        Ok(())
    }

    pub fn delete(&self, provider: AgentProvider) -> AppResult<()> {
        for id in Self::ids(provider) {
            let _ = self.app.ai_secrets().delete(&id);
        }
        self.drop_legacy(provider);
        Ok(())
    }

    pub fn status(&self) -> Vec<AiKeyStatus> {
        AgentProvider::ALL
            .into_iter()
            .map(|p| AiKeyStatus {
                provider: p.as_str().into(),
                has_key: if p.needs_key() {
                    self.get(p).ok().flatten().is_some()
                } else {
                    true
                },
            })
            .collect()
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
    use std::sync::Arc;

    #[test]
    fn sqlite_roundtrip_and_status() {
        let app = Arc::new(AppServices::with_secrets(
            mystique_core::AppDatabase::open_in_memory().unwrap(),
            mystique_core::ConnectionManager::bootstrap(),
            mystique_core::SecretStore::memory(),
        ));
        let store = SecretStore::new(app.clone());
        assert!(
            !store
                .status()
                .iter()
                .find(|s| s.provider == "openai")
                .unwrap()
                .has_key
        );
        store.set(AgentProvider::Openai, " sk-live ").unwrap();
        assert_eq!(
            store.get(AgentProvider::Openai).unwrap().as_deref(),
            Some("sk-live")
        );
        let row = app.ai_secrets().get("openai").unwrap().expect("row");
        assert!(row.ciphertext.starts_with("v1:"));
        assert!(!row.ciphertext.contains("sk-live"));
        store.delete(AgentProvider::Openai).unwrap();
        assert!(store.get(AgentProvider::Openai).unwrap().is_none());
    }
}
