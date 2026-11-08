//! SQLite implementation of local app metadata storage.

mod connection;
mod migrate;
mod repositories;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::AppResult;

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

    /// A panic inside `f` poisons the lock. The next caller takes the
    /// connection back and rolls back whatever transaction the panic left open.
    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let guard = self.conn.lock().unwrap_or_else(|poisoned| {
            let guard = poisoned.into_inner();
            if !guard.is_autocommit() {
                let _ = guard.execute_batch("ROLLBACK");
            }
            self.conn.clear_poison();
            guard
        });
        f(&guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_mid_transaction_does_not_wedge_the_database() {
        let db = AppDatabase::open_in_memory().unwrap();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = db.with_conn(|c| {
                c.execute_batch(
                    "BEGIN; INSERT INTO app_settings (key, value, updated_at) VALUES ('k', 'v', 'now');",
                )?;
                panic!("boom");
                #[allow(unreachable_code)]
                Ok(())
            });
        }));
        assert!(caught.is_err());
        let value = db
            .with_conn(|c| crate::storage::sqlite::repo::settings::get(c, "k"))
            .expect("lock recovered");
        assert!(value.is_none(), "the open transaction was rolled back");
        db.with_conn(|c| {
            assert!(c.is_autocommit());
            crate::storage::sqlite::repo::settings::set(c, "k", "v2")
        })
        .expect("writes work again");
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
