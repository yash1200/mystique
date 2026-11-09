//! Per-database production (F-03): the marks each profile carries, and the
//! database a session writes to when a name is not qualified.
//!
//! Marks are keyed by profile id, so the desktop, agent and MCP sessions
//! of one profile share them. The app sets them before a session opens and
//! again whenever they change, so an open session follows a new mark at its
//! next write.

use std::collections::{HashMap, HashSet};

use mystique_connector_api::{DatabaseEnvironment, SqlDialect};

use super::ConnectionManager;

#[derive(Default)]
pub(super) struct DatabaseMarks {
    by_profile: std::sync::RwLock<HashMap<String, Vec<DatabaseEnvironment>>>,
    /// Sessions that ran `USE`: the database they are in is no longer the
    /// one they opened with, so unqualified names are unresolved.
    switched: std::sync::Mutex<HashSet<String>>,
}

impl DatabaseMarks {
    pub(super) fn get(&self, profile_key: &str) -> Vec<DatabaseEnvironment> {
        self.by_profile
            .read()
            .ok()
            .and_then(|map| map.get(profile_key).cloned())
            .unwrap_or_default()
    }

    pub(super) fn switched(&self, session_id: &str) -> bool {
        self.switched
            .lock()
            .map(|set| set.contains(session_id))
            .unwrap_or(true)
    }

    pub(super) fn forget_session(&self, session_id: &str) {
        if let Ok(mut set) = self.switched.lock() {
            set.remove(session_id);
        }
    }
}

impl ConnectionManager {
    /// The per-database environment marks of a saved profile. Open sessions
    /// of that profile use them from their next write.
    pub fn set_database_marks(&self, profile_id: &str, marks: Vec<DatabaseEnvironment>) {
        let id = profile_id.trim();
        if id.is_empty() {
            return;
        }
        if let Ok(mut map) = self.database_marks.by_profile.write() {
            if marks.is_empty() {
                map.remove(id);
            } else {
                map.insert(id.to_string(), marks);
            }
        }
    }

    /// Remember that a batch switched the session's database (`USE`).
    pub(super) fn note_database_switch(&self, session_id: &str, sql: &str, dialect: SqlDialect) {
        if !mystique_connector_api::sql_switches_database(sql, dialect) {
            return;
        }
        if let Ok(mut set) = self.database_marks.switched.lock() {
            set.insert(session_id.to_string());
        }
    }
}
