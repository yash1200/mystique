//! r2d2 connection manager for a SQLite file.
//!
//! Replaces the `r2d2_sqlite` crate, whose releases lag behind `rusqlite`
//! (we need the SQLite bundled with rusqlite 0.41). Behaviour matches
//! `r2d2_sqlite` 0.25: each connection is opened with the configured flags,
//! the health check is an empty batch (no query against the database), and a
//! connection is never reported broken.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

#[derive(Debug)]
pub(crate) struct SqliteConnectionManager {
    path: PathBuf,
    flags: OpenFlags,
}

impl SqliteConnectionManager {
    pub(crate) fn file(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            flags: OpenFlags::default(),
        }
    }

    pub(crate) fn with_flags(self, flags: OpenFlags) -> Self {
        Self { flags, ..self }
    }
}

impl r2d2::ManageConnection for SqliteConnectionManager {
    type Connection = Connection;
    type Error = rusqlite::Error;

    fn connect(&self) -> Result<Connection, rusqlite::Error> {
        Connection::open_with_flags(&self.path, self.flags)
    }

    fn is_valid(&self, conn: &mut Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch("")
    }

    fn has_broken(&self, _: &mut Connection) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use r2d2::ManageConnection;

    #[test]
    fn pooled_connections_share_the_file() {
        let path = crate::temp_db_path("manager-share");
        let manager = SqliteConnectionManager::file(&path).with_flags(
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        );
        let pool = r2d2::Pool::builder()
            .max_size(2)
            .build(manager)
            .expect("pool");
        let a = pool.get().expect("first");
        let b = pool.get().expect("second");
        a.execute_batch("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (7);")
            .expect("write");
        let x: i64 = b
            .query_row("SELECT x FROM t", [], |r| r.get(0))
            .expect("read");
        assert_eq!(x, 7);
        drop((a, b));
        drop(pool);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_only_flags_reject_writes_and_missing_files() {
        let missing = crate::temp_db_path("manager-missing");
        let ro = SqliteConnectionManager::file(&missing)
            .with_flags(OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX);
        assert!(ro.connect().is_err(), "read-only open must not create");
        assert!(!missing.exists());

        let path = crate::temp_db_path("manager-ro");
        Connection::open(&path)
            .and_then(|c| c.execute_batch("CREATE TABLE t (x INTEGER)"))
            .expect("seed");
        let ro = SqliteConnectionManager::file(&path)
            .with_flags(OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX);
        let mut conn = ro.connect().expect("open read-only");
        ro.is_valid(&mut conn).expect("valid");
        assert!(!ro.has_broken(&mut conn));
        assert!(conn.execute("INSERT INTO t VALUES (1)", []).is_err());
        drop(conn);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sql_load_extension_stays_disabled() {
        let path = crate::temp_db_path("manager-ext");
        let manager = SqliteConnectionManager::file(&path)
            .with_flags(OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE);
        let conn = manager.connect().expect("open");
        let err = conn
            .query_row("SELECT load_extension('mystique_missing_ext')", [], |_| {
                Ok(())
            })
            .expect_err("load_extension must be refused");
        assert!(err.to_string().contains("not authorized"), "{err}");
        drop(conn);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn bundled_sqlite_is_patched_for_rustsec_2026_0459() {
        let conn = Connection::open_in_memory().expect("open");
        let version: String = conn
            .query_row("SELECT sqlite_version()", [], |r| r.get(0))
            .expect("version");
        let parts: Vec<u32> = version
            .split('.')
            .map(|p| p.parse().expect("numeric version part"))
            .collect();
        assert!(
            parts.as_slice() >= [3, 54, 0].as_slice(),
            "bundled SQLite {version} is older than 3.54.0"
        );
    }
}
