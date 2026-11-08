//! The OS keychain, service `dev.mystique.db`.
//!
//! Since the vault, the app keeps one item here: `vault.v1`, the vault key.
//! Older builds kept one item per secret. Those are read once each to move
//! them into the vault, then deleted. Deletes and listing never show UI: on
//! macOS they run with keychain user interaction off and skip whatever would
//! need a prompt.
//!
//! Keychain calls are serialized, so turning interaction off for one call
//! never hides a prompt another thread is about to show.

use std::sync::{Mutex, MutexGuard};

use super::vault::{KeySource, Prompt, VaultError, VaultKey};
use super::SecretBackend;
use crate::error::{AppError, AppResult};

pub(crate) const SERVICE: &str = "dev.mystique.db";
pub const VAULT_ACCOUNT: &str = "vault.v1";

pub(crate) const LOCKED_MESSAGE: &str = "Saved passwords are locked. Open a connection in Mystique once (or use Settings > Retry keychain), then retry.";

static KEYCHAIN: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    KEYCHAIN.lock().unwrap_or_else(|p| p.into_inner())
}

#[cfg(target_os = "macos")]
type Quiet = Option<security_framework::os::macos::keychain::KeychainUserInteractionLock>;
#[cfg(not(target_os = "macos"))]
type Quiet = ();

/// Turn keychain UI off for the guard's lifetime when no prompt is allowed.
/// `None` when it could not be turned off: the caller must not go on.
fn quiet(prompt: Prompt) -> Option<Quiet> {
    #[cfg(target_os = "macos")]
    {
        use security_framework::os::macos::keychain::SecKeychain;
        match prompt {
            Prompt::Allowed => Some(None),
            Prompt::Never => SecKeychain::disable_user_interaction().ok().map(Some),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = prompt;
        Some(())
    }
}

fn entry(account: &str) -> AppResult<keyring::Entry> {
    ensure_native_keyring()?;
    keyring::Entry::new(SERVICE, account).map_err(secret_store_err)
}

/// keyring 3 has no default store. Missing features → in-process mock →
/// save appears to work then the secret is gone after restart.
fn ensure_native_keyring() -> AppResult<()> {
    use keyring::credential::CredentialPersistence;
    match keyring::default::default_credential_builder().persistence() {
        CredentialPersistence::UntilDelete | CredentialPersistence::UntilReboot => Ok(()),
        CredentialPersistence::EntryOnly | CredentialPersistence::ProcessOnly => {
            Err(secret_store_err(
                "keyring is using a non-persistent store; enable apple-native, windows-native, or sync-secret-service",
            ))
        }
        _ => Ok(()),
    }
}

fn secret_store_err(err: impl std::fmt::Display) -> AppError {
    AppError::SecretStore(format!(
        "OS secret store unavailable ({err}). Mystique will not store passwords in the app database."
    ))
}

/// The vault key in the keychain. Created on first use; a write never prompts.
pub struct KeychainKeySource;

impl KeySource for KeychainKeySource {
    fn load(&self, prompt: Prompt) -> Result<VaultKey, VaultError> {
        let refused = |e: &dyn std::fmt::Display| match prompt {
            Prompt::Never => VaultError::Locked,
            Prompt::Allowed => VaultError::Denied(format!("keychain: {e}")),
        };
        let _serial = serial();
        let Some(_quiet) = quiet(prompt) else {
            return Err(VaultError::Locked);
        };
        let entry = entry(VAULT_ACCOUNT).map_err(|e| VaultError::Denied(e.to_string()))?;
        match entry.get_password() {
            Ok(text) => VaultKey::decode(&text).ok_or_else(|| {
                VaultError::Denied("the vault key in the keychain is not a 32-byte key".into())
            }),
            // Never on an error: a denied read must not replace the key.
            Err(keyring::Error::NoEntry) => {
                let key = VaultKey::random();
                entry.set_password(&key.encode()).map_err(|e| refused(&e))?;
                Ok(key)
            }
            Err(e) => Err(refused(&e)),
        }
    }

    fn name(&self) -> &'static str {
        "keychain"
    }
}

/// Items from builds before the vault.
pub(crate) struct KeyringBackend;

impl SecretBackend for KeyringBackend {
    fn get(&self, account: &str, prompt: Prompt) -> AppResult<Option<String>> {
        let _serial = serial();
        let Some(_quiet) = quiet(prompt) else {
            return Err(AppError::SecretStore(LOCKED_MESSAGE.into()));
        };
        match entry(account)?.get_password() {
            Ok(p) if !p.is_empty() => Ok(Some(p)),
            Ok(_) => Ok(None),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) if prompt == Prompt::Never => Err(AppError::SecretStore(LOCKED_MESSAGE.into())),
            Err(e) => Err(secret_store_err(e)),
        }
    }

    fn delete(&self, account: &str) -> AppResult<bool> {
        let _serial = serial();
        platform::delete(account)
    }

    fn list(&self) -> AppResult<Vec<String>> {
        let _serial = serial();
        platform::list()
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
    use security_framework::os::macos::keychain::{SecKeychain, SecPreferencesDomain};

    use super::{secret_store_err, SERVICE};
    use crate::error::AppResult;

    const NOT_FOUND: i32 = -25300;

    /// The keychain the keyring crate uses.
    fn user_keychain() -> AppResult<SecKeychain> {
        SecKeychain::default_for_domain(SecPreferencesDomain::User).map_err(secret_store_err)
    }

    /// `SecItemDelete` reads no secret. With interaction off, an item that
    /// would need a prompt fails and is skipped.
    pub(super) fn delete(account: &str) -> AppResult<bool> {
        let keychain = user_keychain()?;
        let Ok(_quiet) = SecKeychain::disable_user_interaction() else {
            return Ok(false);
        };
        let deleted = ItemSearchOptions::new()
            .keychains(&[keychain])
            .class(ItemClass::generic_password())
            .service(SERVICE)
            .account(account)
            .delete();
        Ok(deleted.is_ok())
    }

    /// Account names only. Attributes are not secret, so no prompt.
    pub(super) fn list() -> AppResult<Vec<String>> {
        let keychain = user_keychain()?;
        let Ok(_quiet) = SecKeychain::disable_user_interaction() else {
            return Ok(Vec::new());
        };
        let found = ItemSearchOptions::new()
            .keychains(&[keychain])
            .class(ItemClass::generic_password())
            .service(SERVICE)
            .load_attributes(true)
            .limit(Limit::All)
            .search();
        let results = match found {
            Ok(results) => results,
            Err(e) if e.code() == NOT_FOUND => return Ok(Vec::new()),
            Err(e) => return Err(secret_store_err(e)),
        };
        let mut accounts: Vec<String> = results
            .iter()
            .filter_map(|r| r.simplify_dict())
            .filter_map(|attrs| attrs.get("acct").cloned())
            .collect();
        accounts.sort();
        accounts.dedup();
        Ok(accounts)
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use crate::error::AppResult;

    /// Windows and Secret Service do not prompt per item.
    pub(super) fn delete(account: &str) -> AppResult<bool> {
        match super::entry(account)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// keyring cannot enumerate items here. Orphans stay until deleted by name.
    pub(super) fn list() -> AppResult<Vec<String>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_keyring_is_not_the_in_process_mock() {
        use keyring::credential::CredentialPersistence;
        assert!(
            !matches!(
                keyring::default::default_credential_builder().persistence(),
                CredentialPersistence::EntryOnly | CredentialPersistence::ProcessOnly
            ),
            "keyring 3 needs apple-native / windows-native / sync-secret-service or passwords vanish on restart"
        );
    }
}
