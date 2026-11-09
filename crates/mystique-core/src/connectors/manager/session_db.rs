//! The database a session's unqualified names go to: the one it opened
//! with, until a `USE` moves it. The row preview reads it to name an
//! `INSERT`'s columns.

use std::collections::HashSet;

use mystique_connector_api::SqlDialect;

use super::ConnectionManager;

/// Sessions that ran `USE`: the database they are in is no longer the one
/// they opened with, so unqualified names are unresolved.
#[derive(Default)]
pub(super) struct DatabaseSwitches(std::sync::Mutex<HashSet<String>>);

impl DatabaseSwitches {
    fn switched(&self, session_id: &str) -> bool {
        self.0
            .lock()
            .map(|set| set.contains(session_id))
            .unwrap_or(true)
    }

    pub(super) fn forget_session(&self, session_id: &str) {
        if let Ok(mut set) = self.0.lock() {
            set.remove(session_id);
        }
    }
}

impl ConnectionManager {
    /// The database unqualified names in this session go to. `None` when a
    /// `USE` moved it, so a guess would not be safe.
    pub(super) async fn session_database(&self, session_id: &str) -> Option<String> {
        if self.database_switches.switched(session_id) {
            return None;
        }
        let guard = self.sessions.read().await;
        guard.get(session_id)?.default_database.clone()
    }

    /// Remember that a batch switched the session's database (`USE`).
    pub(super) fn note_database_switch(&self, session_id: &str, sql: &str, dialect: SqlDialect) {
        if !mystique_connector_api::sql_switches_database(sql, dialect) {
            return;
        }
        if let Ok(mut set) = self.database_switches.0.lock() {
            set.insert(session_id.to_string());
        }
    }
}
