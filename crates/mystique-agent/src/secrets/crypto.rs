//! Old AI key wrapping: AES-256-GCM under a constant compiled into the binary,
//! no AAD. Kept only to unwrap old `ai_secrets` rows on their way into the
//! vault. Nothing new is wrapped this way.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

use mystique_core::{AppError, AppResult};

// Historical key id — do not change; existing wrapped secrets depend on it.
const PEPPER: &[u8; 32] = b"mystique-db.ai.secret.v1.pepper!";

pub(crate) fn unwrap_legacy(blob: &str) -> AppResult<String> {
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

/// What older builds wrote. Tests use it to plant old rows.
#[cfg(test)]
pub(crate) fn wrap_legacy(plain: &str) -> AppResult<String> {
    use rand::RngCore;
    let cipher = Aes256Gcm::new_from_slice(PEPPER)
        .map_err(|e| AppError::Internal(format!("aes key: {e}")))?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plain.as_bytes())
        .map_err(|e| AppError::Internal(format!("encrypt: {e}")))?;
    let mut packed = nonce_bytes.to_vec();
    packed.extend_from_slice(&ct);
    Ok(format!("v1:{}", B64.encode(packed)))
}

#[cfg(test)]
mod tests {
    use super::{unwrap_legacy, wrap_legacy};

    #[test]
    fn unwraps_what_older_builds_wrote() {
        let blob = wrap_legacy("sk-test-key").unwrap();
        assert!(blob.starts_with("v1:"));
        assert_eq!(unwrap_legacy(&blob).unwrap(), "sk-test-key");
    }

    #[test]
    fn rejects_garbage() {
        assert!(unwrap_legacy("nope").is_err());
        assert!(unwrap_legacy("v1:%%%%").is_err());
    }
}
