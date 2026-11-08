use rusqlite::Connection;

use crate::domain::now_iso;
use crate::error::AppResult;

const SCHEMA_SQL: &str = include_str!("../../../migrations/001_init.sql");
const V2_SQL: &str = include_str!("../../../migrations/002_saved_query_favorite.sql");
const V3_SQL: &str = include_str!("../../../migrations/003_connection_favorite.sql");
const V4_SQL: &str = include_str!("../../../migrations/004_ssh_tunnel.sql");
const V5_SQL: &str = include_str!("../../../migrations/005_grid_layout.sql");
const V6_SQL: &str = include_str!("../../../migrations/006_query_history.sql");
const V7_SQL: &str = include_str!("../../../migrations/007_grid_saves.sql");
const V9_SQL: &str = include_str!("../../../migrations/009_connection_defaults.sql");

/// Current schema version. Fresh installs apply 001 then later files.
/// Older builds that ran incremental files 2–6 were already at the 001
/// schema. A stamp of 6 without `query_history.source` is that build and
/// collapses to 1 so 002+ can apply. A real version 6 has `source`.
const SCHEMA_VERSION: i32 = 9;

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

fn table_columns(conn: &Connection, table: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
    Ok(names.collect::<Result<_, _>>()?)
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
    } else if is_pre_reset_stamp(conn, current)? {
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

    if current < 5 {
        conn.execute_batch(V5_SQL)?;
        stamp(conn, 5)?;
    }

    if current < 6 {
        if table_exists(conn, "query_history")? && !column_exists(conn, "query_history", "source")?
        {
            conn.execute_batch(V6_SQL)?;
        }
        stamp(conn, 6)?;
    }

    if current < 7 {
        conn.execute_batch(V7_SQL)?;
        stamp(conn, 7)?;
    }

    // 008 belongs to the grid column widths.
    if current < 9 {
        if table_exists(conn, "connections")? {
            rebuild_connections(conn)?;
        }
        stamp(conn, 9)?;
    }

    // Dropped leftover from earlier builds. Live ER is not stored here.
    conn.execute("DROP TABLE IF EXISTS er_diagrams", [])?;

    Ok(())
}

/// Run 009 with foreign keys off, so dropping the old table does not cascade
/// into saved queries and history. Keeps the AUTOINCREMENT high-water mark so
/// a deleted connection id is never handed out again.
fn rebuild_connections(conn: &Connection) -> AppResult<()> {
    let fk_on: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    conn.execute_batch("PRAGMA foreign_keys = OFF")?;
    let rebuilt = (|| -> AppResult<()> {
        conn.execute_batch("BEGIN")?;
        let seq: Option<i64> = conn
            .query_row(
                "SELECT seq FROM sqlite_sequence WHERE name = 'connections'",
                [],
                |row| row.get(0),
            )
            .ok();
        conn.execute_batch(V9_SQL)?;
        // Copy the columns both tables have. Very old files lack some; those
        // take the new defaults.
        let mut shared = Vec::new();
        for column in table_columns(conn, "connections_new")? {
            if column_exists(conn, "connections", &column)? {
                shared.push(column);
            }
        }
        let shared = shared.join(", ");
        conn.execute_batch(&format!(
            "INSERT INTO connections_new ({shared}) SELECT {shared} FROM connections;
             DROP TABLE connections;
             ALTER TABLE connections_new RENAME TO connections;
             CREATE INDEX IF NOT EXISTS idx_connections_engine ON connections (engine);
             CREATE INDEX IF NOT EXISTS idx_connections_name
               ON connections (name COLLATE NOCASE);
             CREATE INDEX IF NOT EXISTS idx_connections_favorite
               ON connections (favorite, name COLLATE NOCASE);"
        ))?;
        if let Some(seq) = seq {
            conn.execute(
                "UPDATE sqlite_sequence SET seq = MAX(seq, ?1) WHERE name = 'connections'",
                [seq],
            )?;
            conn.execute(
                "INSERT INTO sqlite_sequence (name, seq) SELECT 'connections', ?1
                 WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = 'connections')",
                [seq],
            )?;
        }
        let broken: i64 =
            conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        if broken > 0 {
            return Err(crate::error::AppError::Invalid(
                "connections rebuild broke a foreign key".into(),
            ));
        }
        conn.execute_batch("COMMIT")?;
        Ok(())
    })();
    if rebuilt.is_err() && !conn.is_autocommit() {
        let _ = conn.execute_batch("ROLLBACK");
    }
    if fk_on != 0 {
        conn.execute_batch("PRAGMA foreign_keys = ON")?;
    }
    rebuilt
}

/// Builds before the schema reset stored the 001 file as version 6.
/// A database this release migrated has `query_history.source`.
fn is_pre_reset_stamp(conn: &Connection, current: i32) -> AppResult<bool> {
    if current > SCHEMA_VERSION {
        return Ok(true);
    }
    Ok(current == 6 && !column_exists(conn, "query_history", "source")?)
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
        assert_eq!(version, SCHEMA_VERSION);
        assert!(table_exists(&conn, "grid_saves").unwrap());
        assert!(!table_exists(&conn, "grid_edit_history").unwrap());
        assert!(column_exists(&conn, "query_history", "source").unwrap());
        assert!(column_exists(&conn, "query_history", "database").unwrap());
        assert!(column_exists(&conn, "query_history", "affected_rows").unwrap());
        assert!(column_exists(&conn, "query_history", "duration_ms").unwrap());

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
        assert_eq!(version, SCHEMA_VERSION);
        let n: i32 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        // Every version but 008, which lives on another branch until merge.
        assert_eq!(n, 8);
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
        assert_eq!(version, SCHEMA_VERSION);
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
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn migrate_adds_query_history_columns_on_v5() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(
            "
            CREATE TABLE schema_migrations (
              version INTEGER PRIMARY KEY,
              applied_at TEXT NOT NULL
            );
            INSERT INTO schema_migrations (version, applied_at) VALUES (5, 't');
            CREATE TABLE query_history (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              connection_id INTEGER,
              engine TEXT,
              body TEXT NOT NULL,
              language TEXT,
              success INTEGER NOT NULL,
              row_count INTEGER,
              duration_ms INTEGER,
              error_message TEXT,
              ran_at TEXT NOT NULL
            );
            INSERT INTO query_history (body, success, duration_ms, ran_at)
            VALUES ('select 1', 1, 3, '2020-01-01T00:00:00.000Z');
            ",
        )
        .unwrap();
        migrate(&conn).unwrap();
        assert!(column_exists(&conn, "query_history", "source").unwrap());
        let source: String = conn
            .query_row("SELECT source FROM query_history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(source, "ui");
        let duration: i64 = conn
            .query_row("SELECT duration_ms FROM query_history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(duration, 3);
        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    fn column_default(conn: &Connection, table: &str, column: &str) -> Option<String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let name: String = row.get(1).unwrap();
            if name == column {
                return row.get(4).unwrap();
            }
        }
        None
    }

    #[test]
    fn migrate_009_turns_the_read_only_default_off_and_keeps_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrate(&conn).unwrap();
        // Put the database back at version 8: the old table shape, DEFAULT 1.
        conn.execute_batch(
            "
            PRAGMA foreign_keys = OFF;
            DROP TABLE connections;
            CREATE TABLE connections (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              name TEXT NOT NULL,
              engine TEXT NOT NULL,
              color_id TEXT NOT NULL DEFAULT 'slate',
              environment TEXT NOT NULL DEFAULT 'local',
              read_only INTEGER NOT NULL DEFAULT 1,
              notes TEXT,
              plugin_config TEXT NOT NULL DEFAULT '{}',
              secret_ref TEXT,
              last_connected_at TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              favorite INTEGER NOT NULL DEFAULT 0,
              ssh_config TEXT NOT NULL DEFAULT '{}',
              ssh_secret_ref TEXT
            );
            PRAGMA foreign_keys = ON;
            DELETE FROM schema_migrations WHERE version = 9;
            INSERT INTO schema_migrations (version, applied_at) VALUES (8, 't');
            INSERT INTO connections (id, name, engine, read_only, created_at, updated_at)
              VALUES (1, 'ro', 'sqlite', 1, 'now', 'now'),
                     (2, 'rw', 'sqlite', 0, 'now', 'now'),
                     (5, 'gone', 'sqlite', 1, 'now', 'now');
            DELETE FROM connections WHERE id = 5;
            INSERT INTO saved_queries (connection_id, name, body, created_at, updated_at)
              VALUES (1, 'q', 'select 1', 'now', 'now');
            ",
        )
        .unwrap();
        assert_eq!(
            column_default(&conn, "connections", "read_only").as_deref(),
            Some("1")
        );

        migrate(&conn).unwrap();
        migrate(&conn).unwrap(); // idempotent

        assert_eq!(
            column_default(&conn, "connections", "read_only").as_deref(),
            Some("0")
        );
        let stored: Vec<(i64, i64)> = conn
            .prepare("SELECT id, read_only FROM connections ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(stored, vec![(1, 1), (2, 0)], "existing rows were flipped");
        let timeout: Option<i64> = conn
            .query_row(
                "SELECT query_timeout_secs FROM connections WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(timeout, None, "a migrated profile follows the app setting");
        // Saved queries survived the drop (foreign keys were off).
        let saved: i64 = conn
            .query_row("SELECT COUNT(*) FROM saved_queries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(saved, 1);
        // The deleted id is not reused.
        conn.execute(
            "INSERT INTO connections (name, engine, created_at, updated_at)
             VALUES ('new', 'sqlite', 'now', 'now')",
            [],
        )
        .unwrap();
        let (id, read_only): (i64, i64) = conn
            .query_row(
                "SELECT id, read_only FROM connections WHERE name = 'new'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(id, 6);
        assert_eq!(read_only, 0, "a new row defaults to read-only off");
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1, "foreign keys were left off");
        // Cascades still point at the rebuilt table.
        conn.execute("DELETE FROM connections WHERE id = 1", [])
            .unwrap();
        let saved: i64 = conn
            .query_row("SELECT COUNT(*) FROM saved_queries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(saved, 0);
    }
}
