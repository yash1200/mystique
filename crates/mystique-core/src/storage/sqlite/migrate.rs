use rusqlite::Connection;

use crate::domain::now_iso;
use crate::error::AppResult;

const SCHEMA_SQL: &str = include_str!("../../../migrations/001_init.sql");

/// Baseline schema version. Fresh installs apply `001_init.sql` once.
/// Older builds that ran incremental files 2–6 are already at this schema;
/// their version stamp is collapsed back to 1 so future files can start at 2.
const SCHEMA_VERSION: i32 = 1;

/// Apply the current schema to a new database. Existing databases are left as-is.
pub fn migrate(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_migrations (
          version    INTEGER PRIMARY KEY,
          applied_at TEXT    NOT NULL
        );
        ",
    )?;

    let current: i32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;

    if current == 0 {
        conn.execute_batch(SCHEMA_SQL)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            (SCHEMA_VERSION, now_iso()),
        )?;
        return Ok(());
    }

    if current > SCHEMA_VERSION {
        conn.execute("DELETE FROM schema_migrations", [])?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            (SCHEMA_VERSION, now_iso()),
        )?;
    }

    // Dropped leftover from earlier builds. Live ER is not stored here.
    conn.execute("DROP TABLE IF EXISTS er_diagrams", [])?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_core_tables() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap(); // idempotent

        let n: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='connections'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);

        let secrets: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='ai_secrets'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(secrets, 1);

        conn.execute(
            "INSERT INTO connections (name, engine, color_id, environment, read_only, plugin_config, created_at, updated_at)
             VALUES ('t', 'sqlite', 'slate', 'local', 1, '{}', 'now', 'now')",
            [],
        )
        .unwrap();
        let conn_type: String = conn
            .query_row("SELECT typeof(id) FROM connections", [], |r| r.get(0))
            .unwrap();
        assert_eq!(conn_type, "integer");

        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, 1);

        let groups: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='query_groups'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(groups, 0);

        let er: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='er_diagrams'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(er, 0);
    }

    #[test]
    fn migrate_drops_legacy_er_diagrams() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrate(&conn).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE er_diagrams (
              id INTEGER PRIMARY KEY,
              name TEXT NOT NULL
            );
            ",
        )
        .unwrap();
        migrate(&conn).unwrap();
        let er: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='er_diagrams'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(er, 0);
    }

    #[test]
    fn migrate_collapses_legacy_version_stamps() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(
            "
            CREATE TABLE schema_migrations (
              version INTEGER PRIMARY KEY,
              applied_at TEXT NOT NULL
            );
            INSERT INTO schema_migrations (version, applied_at) VALUES (6, 't');
            ",
        )
        .unwrap();
        migrate(&conn).unwrap();
        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, 1);
        let n: i32 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }
}
