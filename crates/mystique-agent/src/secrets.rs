//! Encrypted API keys in the local SQLite `ai_secrets` table.
//!
//! No OS keychain — macOS Keychain Access prompts on every read. Keys are
//! AES-256-GCM blobs. The frontend never sees plaintext.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use mystique_core::{AppError, AppResult, AppServices};

use crate::providers::AgentProvider;

/// App-local wrapping key. Not a user password — stops casual inspection of
/// `mystique.db` without the binary. Do not treat this as multi-user isolation.
const PEPPER: &[u8; 32] = b"mystique-db.ai.secret.v1.pepper!";

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
        [
            AgentProvider::Openai,
            AgentProvider::Anthropic,
            AgentProvider::Google,
            AgentProvider::Deepseek,
            AgentProvider::Openrouter,
            AgentProvider::Ollama,
        ]
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
        let mut map = read_legacy(path);
        let mut found = None;
        for id in Self::ids(provider) {
            if let Some(value) = map.remove(&id) {
                if !value.trim().is_empty() && found.is_none() {
                    found = Some(value);
                }
            }
        }
        if found.is_some() {
            let _ = write_legacy(path, &map);
        }
        found
    }

    fn drop_legacy(&self, provider: AgentProvider) {
        let Some(path) = self.legacy_file.as_ref() else {
            return;
        };
        let mut map = read_legacy(path);
        for id in Self::ids(provider) {
            map.remove(&id);
        }
        let _ = write_legacy(path, &map);
    }
}

fn encrypt(plain: &str) -> AppResult<String> {
    let cipher = Aes256Gcm::new_from_slice(PEPPER)
        .map_err(|e| AppError::Internal(format!("aes key: {e}")))?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plain.as_bytes())
        .map_err(|e| AppError::Internal(format!("encrypt: {e}")))?;
    let mut packed = Vec::with_capacity(12 + ct.len());
    packed.extend_from_slice(&nonce_bytes);
    packed.extend_from_slice(&ct);
    Ok(format!("v1:{}", B64.encode(packed)))
}

fn decrypt(blob: &str) -> AppResult<String> {
    let raw = blob
        .strip_prefix("v1:")
        .ok_or_else(|| AppError::Internal("unsupported secret encoding".into()))?;
    let packed = B64
        .decode(raw)
        .map_err(|e| AppError::Internal(format!("secret b64: {e}")))?;
    if packed.len() < 13 {
        return Err(AppError::Internal("secret blob too short".into()));
    }
    let (nonce_bytes, ct) = packed.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(PEPPER)
        .map_err(|e| AppError::Internal(format!("aes key: {e}")))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plain = cipher
        .decrypt(nonce, ct)
        .map_err(|_| AppError::Internal("could not decrypt API key".into()))?;
    String::from_utf8(plain).map_err(|_| AppError::Internal("API key is not UTF-8".into()))
}

fn read_legacy(path: &Path) -> HashMap<String, String> {
    let raw = fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write_legacy(path: &Path, map: &HashMap<String, String>) -> AppResult<()> {
    if map.is_empty() {
        let _ = fs::remove_file(path);
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| AppError::Internal(format!("secrets dir: {e}")))?;
    }
    let json = serde_json::to_string(map)
        .map_err(|e| AppError::Internal(format!("secrets json: {e}")))?;
    fs::write(path, json).map_err(|e| AppError::Internal(format!("secrets write: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let blob = encrypt("sk-test-key").unwrap();
        assert!(blob.starts_with("v1:"));
        assert_ne!(blob, "sk-test-key");
        assert_eq!(decrypt(&blob).unwrap(), "sk-test-key");
    }

    #[test]
    fn encrypt_uses_fresh_nonce() {
        let a = encrypt("same").unwrap();
        let b = encrypt("same").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn decrypt_rejects_garbage() {
        assert!(decrypt("nope").is_err());
        assert!(decrypt("v1:%%%%").is_err());
    }

    #[test]
    fn sqlite_roundtrip_and_status() {
        let app = Arc::new(AppServices::new(
            mystique_core::AppDatabase::open_in_memory().unwrap(),
        ));
        let store = SecretStore::new(app);
        assert!(!store
            .status()
            .iter()
            .find(|s| s.provider == "openai")
            .unwrap()
            .has_key);
        store.set(AgentProvider::Openai, " sk-live ").unwrap();
        assert_eq!(store.get(AgentProvider::Openai).unwrap().as_deref(), Some("sk-live"));
        let row = store
            .app
            .ai_secrets()
            .get("openai")
            .unwrap()
            .expect("row");
        assert!(row.ciphertext.starts_with("v1:"));
        assert!(!row.ciphertext.contains("sk-live"));
        store.delete(AgentProvider::Openai).unwrap();
        assert!(store.get(AgentProvider::Openai).unwrap().is_none());
    }
}
