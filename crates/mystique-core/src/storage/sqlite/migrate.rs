use rusqlite::Connection;

use crate::domain::now_iso;
use crate::error::AppResult;

const SCHEMA_SQL: &str = include_str!("../../../migrations/001_init.sql");
const V2_SQL: &str = include_str!("../../../migrations/002_saved_query_favorite.sql");
const V3_SQL: &str = include_str!("../../../migrations/003_connection_favorite.sql");
const V4_SQL: &str = include_str!("../../../migrations/004_ssh_tunnel.sql");

/// Current schema version. Fresh installs apply 001 then later files.
/// Older builds that ran incremental files 2–6 were already at the 001
/// schema; their stamp is collapsed to 1 so 002+ can apply.
const SCHEMA_VERSION: i32 = 4;

fn stamp(conn: &Connection, version: i32) -> AppResult<()> {
    conn.execute(
        "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
        (version, now_iso()),
    )?;
    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> AppResult<bool> {
    let n: i32 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )?;
    Ok(n > 0)
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> AppResult<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Apply the current schema. Existing databases receive pending versions only.
pub fn migrate(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_migrations (
          version    INTEGER PRIMARY KEY,
          applied_at TEXT    NOT NULL
        );
        ",
    )?;

    let mut current: i32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;

    if current == 0 {
        conn.execute_batch(SCHEMA_SQL)?;
        stamp(conn, 1)?;
        current = 1;
    } else if current > SCHEMA_VERSION {
        // Pre-reset incremental stamps (2–6) already matched 001.
        conn.execute("DELETE FROM schema_migrations", [])?;
        stamp(conn, 1)?;
        current = 1;
    }

    if current < 2 {
        if table_exists(conn, "saved_queries")?
            && !column_exists(conn, "saved_queries", "favorite")?
        {
            conn.execute_batch(V2_SQL)?;
        }
        stamp(conn, 2)?;
    }

    if current < 3 {
        if table_exists(conn, "connections")? && !column_exists(conn, "connections", "favorite")? {
            conn.execute_batch(V3_SQL)?;
        }
        stamp(conn, 3)?;
    }

    if current < 4 {
        if table_exists(conn, "connections")? && !column_exists(conn, "connections", "ssh_config")?
        {
            conn.execute_batch(V4_SQL)?;
        }
        stamp(conn, 4)?;
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

        let ssh_hosts: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='ssh_known_hosts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ssh_hosts, 1);
        assert!(column_exists(&conn, "connections", "ssh_config").unwrap());

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
        assert_eq!(version, 4);

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
        assert_eq!(version, 4);
        let n: i32 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 4);
    }

    #[test]
    fn migrate_adds_saved_query_favorite_on_v1() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(
            "
            CREATE TABLE schema_migrations (
              version INTEGER PRIMARY KEY,
              applied_at TEXT NOT NULL
            );
            INSERT INTO schema_migrations (version, applied_at) VALUES (1, 't');
            CREATE TABLE connections (
              id INTEGER PRIMARY KEY,
              name TEXT NOT NULL,
              engine TEXT NOT NULL,
              color_id TEXT NOT NULL,
              environment TEXT NOT NULL,
              read_only INTEGER NOT NULL,
              plugin_config TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE saved_queries (
              id INTEGER PRIMARY KEY,
              connection_id INTEGER NOT NULL,
              name TEXT NOT NULL,
              body TEXT NOT NULL,
              language TEXT,
              sort_order INTEGER NOT NULL DEFAULT 0,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            ",
        )
        .unwrap();
        migrate(&conn).unwrap();
        assert!(column_exists(&conn, "saved_queries", "favorite").unwrap());
        assert!(column_exists(&conn, "connections", "favorite").unwrap());
        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, 4);
    }

    #[test]
    fn migrate_adds_connection_favorite_on_v2() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(
            "
            CREATE TABLE schema_migrations (
              version INTEGER PRIMARY KEY,
              applied_at TEXT NOT NULL
            );
            INSERT INTO schema_migrations (version, applied_at) VALUES (1, 't');
            INSERT INTO schema_migrations (version, applied_at) VALUES (2, 't');
            CREATE TABLE connections (
              id INTEGER PRIMARY KEY,
              name TEXT NOT NULL,
              engine TEXT NOT NULL,
              color_id TEXT NOT NULL,
              environment TEXT NOT NULL,
              read_only INTEGER NOT NULL,
              plugin_config TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            ",
        )
        .unwrap();
        migrate(&conn).unwrap();
        assert!(column_exists(&conn, "connections", "favorite").unwrap());
        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, 4);
    }
}
