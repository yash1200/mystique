//! The app database schema: one SQL file, no migrations.
//!
//! `PRAGMA user_version` holds [`SCHEMA_VERSION`]. A new database gets the
//! whole schema at once. A database at any other version (an older build's
//! file, which stamped its migrations elsewhere and left this at 0) is not
//! converted: [`super::AppDatabase::open`] moves it aside and starts fresh.

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

const SCHEMA_SQL: &str = include_str!("../../../schema/schema.sql");

/// Bump only together with a new `schema.sql`; there is no upgrade path.
pub const SCHEMA_VERSION: i32 = 1;

/// What an opened database holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaState {
    /// No tables yet.
    Empty,
    /// This build's schema.
    Current,
    /// Another schema: an older build's file, or a newer build's.
    Other(i32),
}

/// `PRAGMA user_version`: 0 on a new or a pre-1 database.
pub fn applied_version(conn: &Connection) -> AppResult<i32> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

pub fn schema_state(conn: &Connection) -> AppResult<SchemaState> {
    let version = applied_version(conn)?;
    if version == SCHEMA_VERSION {
        return Ok(SchemaState::Current);
    }
    let tables: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    Ok(if version == 0 && tables == 0 {
        SchemaState::Empty
    } else {
        SchemaState::Other(version)
    })
}

/// Creates the schema on an empty database; a current one is left as is.
/// Any other schema is an error here: only `open` may replace a file.
pub fn ensure_schema(conn: &Connection) -> AppResult<()> {
    match schema_state(conn)? {
        SchemaState::Current => Ok(()),
        SchemaState::Empty => {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(SCHEMA_SQL)?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            tx.commit()?;
            tracing::info!(version = SCHEMA_VERSION, "app database created");
            Ok(())
        }
        SchemaState::Other(version) => Err(AppError::Invalid(format!(
            "The app database is at schema {version}; this build uses {SCHEMA_VERSION}."
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn
    }

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            == 1
    }

    #[test]
    fn creates_every_table_once() {
        let conn = fresh();
        assert_eq!(schema_state(&conn).unwrap(), SchemaState::Empty);
        ensure_schema(&conn).unwrap();
        ensure_schema(&conn).unwrap(); // a current database is left alone
        assert_eq!(applied_version(&conn).unwrap(), SCHEMA_VERSION);
        for table in [
            "connections",
            "secrets",
            "saved_queries",
            "query_history",
            "workspace_tabs",
            "grid_column_layout",
            "grid_saves",
            "tool_access",
            "agent_chats",
            "agent_messages",
            "app_settings",
            "ssh_known_hosts",
        ] {
            assert!(table_exists(&conn, table), "{table}");
        }
        for gone in ["schema_migrations", "ai_secrets", "er_diagrams"] {
            assert!(!table_exists(&conn, gone), "{gone}");
        }
    }

    #[test]
    fn an_older_file_is_not_touched() {
        let conn = fresh();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT);
             INSERT INTO schema_migrations VALUES (15, 'now');",
        )
        .unwrap();
        assert_eq!(schema_state(&conn).unwrap(), SchemaState::Other(0));
        assert!(ensure_schema(&conn).is_err());
        assert!(!table_exists(&conn, "connections"));
    }

    #[test]
    fn deleting_a_connection_clears_its_grid_rows() {
        let conn = fresh();
        ensure_schema(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO connections (id, name, engine, created_at, updated_at)
               VALUES (7, 'c', 'sqlite', 'now', 'now');
             INSERT INTO grid_column_layout (connection_id, table_id, updated_at)
               VALUES ('7', 't', 'now'), ('session-x', 't', 'now');
             INSERT INTO grid_saves (save_id, connection_id, table_key, undo, created_at)
               VALUES ('s1', '7', 't', '{}', 'now');
             DELETE FROM connections WHERE id = 7;",
        )
        .unwrap();
        let left: Vec<String> = conn
            .prepare("SELECT connection_id FROM grid_column_layout")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(left, ["session-x"]);
        let saves: i64 = conn
            .query_row("SELECT COUNT(*) FROM grid_saves", [], |r| r.get(0))
            .unwrap();
        assert_eq!(saves, 0);
    }

    #[test]
    fn new_connections_default_to_writable_with_a_saved_password() {
        let conn = fresh();
        ensure_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO connections (name, engine, created_at, updated_at)
             VALUES ('c', 'sqlite', 'now', 'now')",
            [],
        )
        .unwrap();
        let (read_only, save_password): (i64, i64) = conn
            .query_row(
                "SELECT read_only, save_password FROM connections",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((read_only, save_password), (0, 1));
    }
}
