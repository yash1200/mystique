//! App-level secrets (AI keys, the MCP token), vault status and the
//! keychain cleanup.

use serde::Serialize;

use crate::domain::is_legacy_keychain_pointer;
use crate::error::AppResult;
use crate::secrets::{Prompt, SecretKind, SecretOwner, SecretStore, VaultStatus, VAULT_ACCOUNT};
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

/// Set once orphaned keychain items were cleaned up for this install.
const PURGED_KEY: &str = "secrets.keychain_purge_v1";

pub struct VaultService<'a> {
    pub(crate) db: &'a AppDatabase,
    pub(crate) secrets: &'a SecretStore,
    pub(crate) prompt: Prompt,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PurgeReport {
    /// False when this build does not use the keychain (file key source).
    pub supported: bool,
    pub deleted: usize,
    /// Items that would have needed a prompt to delete.
    pub skipped: usize,
}

impl VaultService<'_> {
    /// For MCP: fail instead of showing a keychain prompt.
    pub fn without_prompt(self) -> Self {
        Self {
            prompt: Prompt::Never,
            ..self
        }
    }

    pub fn get(&self, kind: SecretKind<'_>) -> AppResult<Option<String>> {
        self.secrets
            .read(self.db, SecretOwner::App, kind, self.prompt)
    }

    pub fn set(&self, kind: SecretKind<'_>, value: &str) -> AppResult<()> {
        let key = self.secrets.unlock(self.prompt)?;
        self.db
            .with_tx(|c| self.secrets.write(c, &key, SecretOwner::App, kind, value))
    }

    pub fn delete(&self, kind: SecretKind<'_>) -> AppResult<()> {
        self.db
            .with_conn(|c| SecretStore::remove(c, SecretOwner::App, kind))
    }

    /// Never loads the key.
    pub fn has(&self, kind: SecretKind<'_>) -> AppResult<bool> {
        self.db
            .with_conn(|c| SecretStore::has(c, SecretOwner::App, kind))
    }

    /// Never loads the key.
    pub fn status(&self) -> VaultStatus {
        self.secrets.status()
    }

    /// Settings > Retry keychain: forget a denial and ask again.
    pub fn retry(&self) -> VaultStatus {
        self.secrets.retry_unlock()
    }

    /// Settings > Clean up old keychain items. Never shows keychain UI.
    pub fn purge_keychain(&self) -> AppResult<PurgeReport> {
        purge_orphans(self.secrets)
    }
}

/// Background work after the key first loads in this process: once per
/// install, delete the keychain items older builds left behind.
pub(crate) fn after_first_unlock(db: &AppDatabase, secrets: &SecretStore) {
    if !secrets.legacy_enabled() {
        return;
    }
    let done = db
        .with_conn(|c| repo::settings::get(c, PURGED_KEY))
        .ok()
        .flatten()
        .is_some();
    if !done && purge_orphans(secrets).is_ok() {
        let _ = db.with_conn(|c| repo::settings::set(c, PURGED_KEY, "1"));
    }
}

/// Delete every keychain item older builds left: `conn.*`, `ai.api_key.*` and
/// `connection.*.password`. No row reads them any more; only the vault key
/// stays. Lists attributes only and deletes with interaction off; anything
/// that would prompt is skipped.
pub(crate) fn purge_orphans(secrets: &SecretStore) -> AppResult<PurgeReport> {
    if !secrets.legacy_enabled() {
        return Ok(PurgeReport::default());
    }
    let mut report = PurgeReport {
        supported: true,
        ..PurgeReport::default()
    };
    for account in secrets.legacy_list()? {
        let ours = account.starts_with("conn.")
            || account.starts_with("ai.api_key.")
            || is_legacy_keychain_pointer(&account);
        if !ours || account == VAULT_ACCOUNT {
            continue;
        }
        if secrets.legacy_delete(&account) {
            report.deleted += 1;
        } else {
            report.skipped += 1;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;
    use crate::application::AppServices;
    use crate::connectors::ConnectionManager;
    use crate::secrets::counting::{CountingKeySource, FakeKeychain};

    fn app() -> (AppServices, Arc<FakeKeychain>) {
        let items = Arc::new(FakeKeychain::default());
        let app = AppServices::with_secrets(
            AppDatabase::open_in_memory().unwrap(),
            ConnectionManager::bootstrap(),
            SecretStore::from_parts(Arc::new(CountingKeySource::default()), Some(items.clone())),
        );
        (app, items)
    }

    #[test]
    fn the_unlock_hook_runs_once_per_process() {
        let (app, _) = app();
        let runs = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&runs);
        app.secrets.on_first_unlock(move || {
            seen.fetch_add(1, Ordering::SeqCst);
        });
        app.vault().set(SecretKind::McpToken, "t").unwrap();
        app.vault().get(SecretKind::McpToken).unwrap();
        app.secrets.retry_unlock();
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn after_first_unlock_purges_once() {
        let (app, items) = app();
        items.plant("ai.api_key.google", "x");
        app.secrets.unlock(Prompt::Allowed).unwrap();

        after_first_unlock(&app.db, &app.secrets);
        assert!(!items.contains("ai.api_key.google"));

        // Once per install: a later orphan waits for the Settings button.
        items.plant("ai.api_key.openai", "x");
        after_first_unlock(&app.db, &app.secrets);
        assert!(items.contains("ai.api_key.openai"));
        let report = app.vault().purge_keychain().unwrap();
        assert_eq!(report.deleted, 1);
        assert!(!items.contains("ai.api_key.openai"));
    }

    #[test]
    fn no_purge_without_the_keychain() {
        let app = AppServices::with_secrets(
            AppDatabase::open_in_memory().unwrap(),
            ConnectionManager::bootstrap(),
            SecretStore::from_parts(Arc::new(crate::secrets::MemoryKeySource::new()), None),
        );
        let report = app.vault().purge_keychain().unwrap();
        assert!(!report.supported);
        after_first_unlock(&app.db, &app.secrets);
        assert!(app
            .db
            .with_conn(|c| repo::settings::get(c, PURGED_KEY))
            .unwrap()
            .is_none());
    }
}
