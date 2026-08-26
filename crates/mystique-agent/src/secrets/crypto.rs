use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;

use mystique_core::{AppError, AppResult};

/// App-local wrapping key. Not a user password — stops casual inspection of
/// `mystique.db` without the binary. Do not treat this as multi-user isolation.
// Historical key id — do not change; existing wrapped secrets depend on it.
const PEPPER: &[u8; 32] = b"mystique-db.ai.secret.v1.pepper!";

pub(crate) fn encrypt(plain: &str) -> AppResult<String> {
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

pub(crate) fn decrypt(blob: &str) -> AppResult<String> {
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

#[cfg(test)]
mod tests {
    use super::{decrypt, encrypt};

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
}
