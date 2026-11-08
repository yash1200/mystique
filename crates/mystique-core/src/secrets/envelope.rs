//! AES-256-GCM envelopes for the `secrets` table.
//!
//! `v1.<nonce>.<ciphertext>`, URL-safe base64 without padding. The AAD names
//! the install, the owner and the kind, so an envelope copied to another row,
//! kind or install does not open.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use rand::RngCore;

use super::vault::VaultKey;
use crate::error::{AppError, AppResult};

const PREFIX: &str = "v1";

/// Bumped when the vault key is rotated. Stored next to each envelope.
pub(crate) const KEY_VERSION: i64 = 1;

const UNREADABLE: &str = "A saved secret could not be decrypted with this build's vault key. \
It was probably saved with another key (another install, or MYSTIQUE_VAULT_KEY_SOURCE=file). \
Edit the connection and enter it again.";

/// Where an envelope lives. All of it goes into the AAD.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Slot<'a> {
    pub install: &'a str,
    pub owner_kind: &'a str,
    pub owner_id: &'a str,
    pub kind: &'a str,
}

fn aad(slot: &Slot<'_>) -> Vec<u8> {
    format!(
        "mystique-secret-v1\0{}\0{}\0{}\0{}",
        slot.install, slot.owner_kind, slot.owner_id, slot.kind
    )
    .into_bytes()
}

fn cipher(key: &VaultKey) -> AppResult<Aes256Gcm> {
    Aes256Gcm::new_from_slice(key.bytes()).map_err(|e| AppError::Internal(format!("aes key: {e}")))
}

pub(crate) fn seal(key: &VaultKey, slot: &Slot<'_>, plaintext: &str) -> AppResult<String> {
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let aad = aad(slot);
    let ct = cipher(key)?
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext.as_bytes(),
                aad: &aad,
            },
        )
        .map_err(|_| AppError::Internal("secret encryption failed".into()))?;
    Ok(format!("{PREFIX}.{}.{}", B64.encode(nonce), B64.encode(ct)))
}

pub(crate) fn open(key: &VaultKey, slot: &Slot<'_>, envelope: &str) -> AppResult<String> {
    let mut parts = envelope.trim().split('.');
    let (Some(PREFIX), Some(nonce), Some(ct), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(AppError::SecretStore(UNREADABLE.into()));
    };
    let nonce = B64
        .decode(nonce)
        .ok()
        .filter(|n| n.len() == 12)
        .ok_or_else(|| AppError::SecretStore(UNREADABLE.into()))?;
    let ct = B64
        .decode(ct)
        .map_err(|_| AppError::SecretStore(UNREADABLE.into()))?;
    let aad = aad(slot);
    let plain = cipher(key)?
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ct,
                aad: &aad,
            },
        )
        .map_err(|_| AppError::SecretStore(UNREADABLE.into()))?;
    String::from_utf8(plain).map_err(|_| AppError::SecretStore(UNREADABLE.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot<'a>(install: &'a str, owner_id: &'a str, kind: &'a str) -> Slot<'a> {
        Slot {
            install,
            owner_kind: "connection",
            owner_id,
            kind,
        }
    }

    #[test]
    fn round_trip() {
        let key = VaultKey::random();
        let here = slot("install-a", "7", "password");
        let env = seal(&key, &here, "s3cret").unwrap();
        assert!(env.starts_with("v1."));
        assert_eq!(env.split('.').count(), 3);
        assert!(!env.contains("s3cret"));
        assert_eq!(open(&key, &here, &env).unwrap(), "s3cret");
        // Fresh nonce each time.
        assert_ne!(env, seal(&key, &here, "s3cret").unwrap());
    }

    #[test]
    fn an_envelope_does_not_open_in_another_slot() {
        let key = VaultKey::random();
        let here = slot("install-a", "7", "password");
        let env = seal(&key, &here, "s3cret").unwrap();
        for other in [
            slot("install-a", "8", "password"),
            slot("install-a", "7", "ssh"),
            slot("install-a", "7", "tls"),
            slot("install-b", "7", "password"),
            Slot {
                owner_kind: "app",
                ..here
            },
        ] {
            assert!(open(&key, &other, &env).is_err(), "{other:?}");
        }
        assert!(open(&VaultKey::random(), &here, &env).is_err(), "other key");
    }

    #[test]
    fn tampering_fails() {
        let key = VaultKey::random();
        let here = slot("i", "1", "password");
        let env = seal(&key, &here, "s3cret").unwrap();
        let parts: Vec<&str> = env.split('.').collect();
        let mut ct = B64.decode(parts[2]).unwrap();
        ct[0] ^= 1;
        let flipped = format!("v1.{}.{}", parts[1], B64.encode(&ct));
        assert!(open(&key, &here, &flipped).is_err());
        let mut nonce = B64.decode(parts[1]).unwrap();
        nonce[0] ^= 1;
        let renonced = format!("v1.{}.{}", B64.encode(&nonce), parts[2]);
        assert!(open(&key, &here, &renonced).is_err());
        assert!(open(&key, &here, &env.replacen("v1.", "v2.", 1)).is_err());
        assert!(open(&key, &here, &format!("{env}.x")).is_err());
        assert!(open(&key, &here, "s3cret").is_err());
    }
}
