use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::domain::{validate_environment, ConnectionProfile, SaveConnectionInput};
use crate::domain::now_iso;

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConnectionProfile> {
    let plugin_config_raw: String = row.get("plugin_config")?;
    let plugin_config: JsonValue =
        serde_json::from_str(&plugin_config_raw).unwrap_or(JsonValue::Object(Default::default()));
    let secret_ref: Option<String> = row.get("secret_ref")?;
    let read_only_i: i64 = row.get("read_only")?;

    Ok(ConnectionProfile {
        id: row.get("id")?,
        name: row.get("name")?,
        engine: row.get("engine")?,
        color_id: row.get("color_id")?,
        environment: row.get("environment")?,
        read_only: read_only_i != 0,
        notes: row.get("notes")?,
        plugin_config,
        password_set: secret_ref.as_ref().is_some_and(|s| !s.is_empty()),
        last_connected_at: row.get("last_connected_at")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

const SELECT_COLS: &str = "
  id, name, engine, color_id, environment, read_only, notes,
  plugin_config, secret_ref, last_connected_at, created_at, updated_at
";

pub fn list_all(conn: &Connection) -> AppResult<Vec<ConnectionProfile>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS} FROM connections ORDER BY name COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn get_by_id(conn: &Connection, id: &str) -> AppResult<ConnectionProfile> {
    conn.query_row(
        &format!("SELECT {SELECT_COLS} FROM connections WHERE id = ?1"),
        params![id],
        map_row,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("connection '{id}'")))
}

/// Create or update a connection. Returns the saved profile.
pub fn save(conn: &Connection, input: SaveConnectionInput) -> AppResult<ConnectionProfile> {
    if input.name.trim().is_empty() {
        return Err(AppError::Invalid("name is required".into()));
    }
    if input.engine.trim().is_empty() {
        return Err(AppError::Invalid("engine is required".into()));
    }
    validate_environment(&input.environment).map_err(AppError::Invalid)?;

    let plugin_config = serde_json::to_string(&input.plugin_config)?;
    let now = now_iso();
    let notes = input
        .notes
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    if let Some(id) = input.id.as_ref().filter(|s| !s.is_empty()) {
        let secret_ref = if input.clear_secret {
            None
        } else if input.secret_ref.is_some() {
            input.secret_ref.clone()
        } else {
            conn.query_row(
                "SELECT secret_ref FROM connections WHERE id = ?1",
                params![id],
                |r| r.get::<_, Option<String>>(0),
            )?
        };

        let n = conn.execute(
            "
            UPDATE connections SET
              name = ?1,
              engine = ?2,
              color_id = ?3,
              environment = ?4,
              read_only = ?5,
              notes = ?6,
              plugin_config = ?7,
              secret_ref = ?8,
              updated_at = ?9
            WHERE id = ?10
            ",
            params![
                input.name.trim(),
                input.engine.trim(),
                input.color_id.trim(),
                input.environment.trim(),
                if input.read_only { 1 } else { 0 },
                notes,
                plugin_config,
                secret_ref,
                now,
                id,
            ],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("connection '{id}'")));
        }
        get_by_id(conn, id)
    } else {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "
            INSERT INTO connections (
              id, name, engine, color_id, environment, read_only, notes,
              plugin_config, secret_ref, last_connected_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11)
            ",
            params![
                id,
                input.name.trim(),
                input.engine.trim(),
                input.color_id.trim(),
                input.environment.trim(),
                if input.read_only { 1 } else { 0 },
                notes,
                plugin_config,
                input.secret_ref,
                now,
                now,
            ],
        )?;
        get_by_id(conn, &id)
    }
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM connections WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(AppError::NotFound(format!("connection '{id}'")));
    }
    Ok(())
}

pub fn touch_last_connected(conn: &Connection, id: &str) -> AppResult<ConnectionProfile> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE connections SET last_connected_at = ?1, updated_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("connection '{id}'")));
    }
    get_by_id(conn, id)
}

/// Returns the raw secret_ref for internal use (e.g. keychain lookup later).
#[allow(dead_code)]
pub fn get_secret_ref(conn: &Connection, id: &str) -> AppResult<Option<String>> {
    conn.query_row(
        "SELECT secret_ref FROM connections WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("connection '{id}'")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::migrate;
    use serde_json::json;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn save_list_delete_connection() {
        let conn = test_db();
        let saved = save(
            &conn,
            SaveConnectionInput {
                id: None,
                name: "local-pg".into(),
                engine: "postgres".into(),
                color_id: "teal".into(),
                environment: "local".into(),
                read_only: true,
                notes: Some("test".into()),
                plugin_config: json!({
                    "host": "127.0.0.1",
                    "port": 5432,
                    "database": "app",
                    "username": "postgres"
                }),
                secret_ref: Some("mystique/conn/test".into()),
                clear_secret: false,
            },
        )
        .unwrap();

        assert!(saved.password_set);
        assert_eq!(saved.engine, "postgres");
        assert_eq!(saved.plugin_config["port"], 5432);

        let all = list_all(&conn).unwrap();
        assert_eq!(all.len(), 1);

        delete(&conn, &saved.id).unwrap();
        assert!(list_all(&conn).unwrap().is_empty());
    }
}
