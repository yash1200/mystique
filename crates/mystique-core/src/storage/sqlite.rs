//! SQLite implementation of local app metadata storage.

mod connection;
mod repositories;
mod schema;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

pub use connection::{db_file_name, open_path};
pub use schema::{applied_version, ensure_schema, schema_state, SchemaState, SCHEMA_VERSION};

/// Process-wide handle to the local app database.
pub struct AppDatabase {
    conn: Mutex<Connection>,
}

impl AppDatabase {
    /// Opens the app database, creating it if needed. A file from an older
    /// build is not converted: it is moved aside (`mystique.db.<time>.bak`) and
    /// a new database takes its place. A newer build's file is left alone.
    pub fn open(path: &Path) -> AppResult<Self> {
        let mut conn = open_path(path)?;
        if let SchemaState::Other(version) = schema_state(&conn)? {
            if version > SCHEMA_VERSION {
                return Err(AppError::Invalid(format!(
                    "This database was written by a newer Mystique (schema {version}). \
                     Update the app to open it."
                )));
            }
            drop(conn);
            let backup = set_aside(path)?;
            tracing::warn!(
                from = version,
                backup = %backup.display(),
                "app database is from an older build; moved aside, starting fresh"
            );
            conn = open_path(path)?;
        }
        ensure_schema(&conn)?;
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
        ensure_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// The schema version this database is at.
    pub fn schema_version(&self) -> AppResult<i32> {
        self.with_conn(applied_version)
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

    /// `f` runs in one transaction: an error rolls back everything it wrote.
    pub fn with_tx<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let out = f(&tx)?;
            tx.commit()?;
            Ok(out)
        })
    }
}

/// Renames the database and its WAL / shared-memory files to
/// `<name>.<timestamp>.bak[-wal|-shm]`. Returns the main file's new path.
fn set_aside(path: &Path) -> AppResult<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| connection::DB_FILE_NAME.to_string());
    let backup = path.with_file_name(format!("{name}.{stamp}.bak"));
    std::fs::rename(path, &backup)?;
    for suffix in ["-wal", "-shm"] {
        let side = path.with_file_name(format!("{name}{suffix}"));
        if side.exists() {
            let _ = std::fs::rename(
                &side,
                path.with_file_name(format!("{name}.{stamp}.bak{suffix}")),
            );
        }
    }
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_database_file_is_moved_aside() {
        let dir = std::env::temp_dir().join(format!("mystique-db-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mystique.db");
        {
            let old = Connection::open(&path).unwrap();
            old.execute_batch(
                "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT);
                 CREATE TABLE connections (id INTEGER PRIMARY KEY, name TEXT);
                 INSERT INTO connections VALUES (1, 'old');",
            )
            .unwrap();
        }
        let db = AppDatabase::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
        let rows: i64 = db
            .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM connections", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(rows, 0, "a fresh database");
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".bak"))
            .collect();
        assert_eq!(backups.len(), 1, "{backups:?}");
        let kept = Connection::open(dir.join(&backups[0])).unwrap();
        let name: String = kept
            .query_row("SELECT name FROM connections", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "old");
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_database_file_is_refused_and_kept() {
        let dir = std::env::temp_dir().join(format!("mystique-db-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mystique.db");
        {
            let newer = Connection::open(&path).unwrap();
            newer
                .execute_batch("CREATE TABLE t (x); PRAGMA user_version = 99;")
                .unwrap();
        }
        assert!(AppDatabase::open(&path).is_err());
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

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
    pub use super::repositories::connections;
    pub use super::repositories::grid_layout;
    pub use super::repositories::history;
    pub use super::repositories::queries;
    pub use super::repositories::secrets;
    pub use super::repositories::settings;
    pub use super::repositories::ssh_hosts;
    pub use super::repositories::tool_access;
    pub use super::repositories::workspace_tabs;
}
