//! Passwords for connections that do not save one.
//!
//! Process memory only: never written to SQLite, the vault, the keychain or a
//! log. A password lands here after a connect with it succeeds, and is gone
//! when the profile changes host, is deleted, or the app quits.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

#[derive(Default)]
struct State {
    passwords: HashMap<String, String>,
    /// Bumped by every forget, so a connect that started before it cannot
    /// put the old password back.
    generation: u64,
}

#[derive(Default)]
pub struct SessionPasswords {
    state: Mutex<State>,
}

/// A typed password waiting for its connect to succeed.
pub struct PendingPassword {
    connection_id: String,
    password: String,
    generation: u64,
}

impl fmt::Debug for PendingPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingPassword")
            .field("connection_id", &self.connection_id)
            .finish_non_exhaustive()
    }
}

impl SessionPasswords {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub(crate) fn get(&self, connection_id: &str) -> Option<String> {
        self.lock().passwords.get(connection_id.trim()).cloned()
    }

    /// Hold a typed password until [`Self::commit`].
    pub(crate) fn pending(&self, connection_id: &str, password: &str) -> PendingPassword {
        PendingPassword {
            connection_id: connection_id.trim().to_string(),
            password: password.to_string(),
            generation: self.lock().generation,
        }
    }

    /// Keep the password for the session, unless the profile changed meanwhile.
    pub(crate) fn commit(&self, pending: PendingPassword) {
        let mut state = self.lock();
        if state.generation == pending.generation && !pending.password.trim().is_empty() {
            state
                .passwords
                .insert(pending.connection_id, pending.password);
        }
    }

    pub(crate) fn forget(&self, connection_id: &str) {
        let mut state = self.lock();
        state.generation += 1;
        state.passwords.remove(connection_id.trim());
    }

    pub(crate) fn clear(&self) {
        let mut state = self.lock();
        state.generation += 1;
        state.passwords.clear();
    }

    pub(crate) fn len(&self) -> usize {
        self.lock().passwords.len()
    }
}

/// Only the count, never an id or a password.
impl fmt::Debug for SessionPasswords {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionPasswords")
            .field("count", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forget_drops_a_connect_that_started_before_it() {
        let store = SessionPasswords::default();
        let pending = store.pending("1", "pw");
        store.forget("1");
        store.commit(pending);
        assert!(store.get("1").is_none());

        store.commit(store.pending("1", "pw"));
        assert_eq!(store.get("1").as_deref(), Some("pw"));
    }

    #[test]
    fn debug_shows_no_secret() {
        let store = SessionPasswords::default();
        store.commit(store.pending("7", "hunter2"));
        let shown = format!("{store:?} {:?}", store.pending("7", "hunter2"));
        assert!(!shown.contains("hunter2"), "{shown}");
    }
}
