//! SQLite implementation of local app metadata storage.

mod connection;
mod migrate;
mod repositories;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

pub use connection::{db_file_name, open_path};
pub use migrate::migrate;

/// Process-wide handle to the local app database.
pub struct AppDatabase {
    conn: Mutex<Connection>,
}

impl AppDatabase {
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = open_path(path)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;
            ",
        )?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let guard = self
            .conn
            .lock()
            .map_err(|e| AppError::Internal(format!("db lock poisoned: {e}")))?;
        f(&guard)
    }
}

pub mod repo {
    pub use super::repositories::agent_chats;
    pub use super::repositories::ai_secrets;
    pub use super::repositories::connections;
    pub use super::repositories::grid_layout;
    pub use super::repositories::history;
    pub use super::repositories::queries;
    pub use super::repositories::settings;
    pub use super::repositories::ssh_hosts;
    pub use super::repositories::workspace_tabs;
}
