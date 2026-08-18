use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::domain::now_iso;
use crate::domain::{
    is_placeholder_secret_ref, validate_environment, ConnectionProfile, SaveConnectionInput,
};
use crate::error::{AppError, AppResult};

/// Normalize stored secret: drop legacy keychain placeholders; keep real passwords.
fn password_from_stored(secret_ref: Option<String>) -> Option<String> {
    secret_ref.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() || is_placeholder_secret_ref(&t) {
            None
        } else {
            Some(t)
        }
    })
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConnectionProfile> {
    let plugin_config_raw: String = row.get("plugin_config")?;
    let plugin_config: JsonValue =
        serde_json::from_str(&plugin_config_raw).unwrap_or(JsonValue::Object(Default::default()));
    let stored: Option<String> = row.get("secret_ref")?;
    let password = password_from_stored(stored);
    let password_set = password.as_ref().is_some_and(|s| !s.is_empty());
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
        password_set,
        password,
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

/// Resolve the password value to persist in `secret_ref` (plaintext for now).
fn resolve_password_to_store(
    conn: &Connection,
    input: &SaveConnectionInput,
    existing_id: Option<&str>,
) -> AppResult<Option<String>> {
    if input.clear_secret {
        return Ok(None);
    }

    // Prefer explicit password field.
    if let Some(pw) = input
        .password
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return Ok(Some(pw.to_string()));
    }

    // Legacy: accept secret_ref only when it is a real secret (not a placeholder path).
    if let Some(sr) = input
        .secret_ref
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        if !is_placeholder_secret_ref(sr) {
            return Ok(Some(sr.to_string()));
        }
        // Placeholder on create → store nothing; on update → keep existing below.
    }

    // Update with blank password → keep previous secret.
    if let Some(id) = existing_id {
        let prev: Option<String> = conn
            .query_row(
                "SELECT secret_ref FROM connections WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        return Ok(password_from_stored(prev));
    }

    Ok(None)
}

/// Create or update a connection. Returns the saved profile (includes password).
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
        let secret_ref = resolve_password_to_store(conn, &input, Some(id))?;

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
        let secret_ref = resolve_password_to_store(conn, &input, None)?;
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
                secret_ref,
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

/// Returns the stored plaintext password (if any).
#[allow(dead_code)]
pub fn get_password(conn: &Connection, id: &str) -> AppResult<Option<String>> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT secret_ref FROM connections WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("connection '{id}'")))?;
    Ok(password_from_stored(stored))
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
                password: Some("s3cret".into()),
                secret_ref: None,
                clear_secret: false,
            },
        )
        .unwrap();

        assert!(saved.password_set);
        assert_eq!(saved.password.as_deref(), Some("s3cret"));
        assert_eq!(saved.engine, "postgres");
        assert_eq!(saved.plugin_config["port"], 5432);

        let all = list_all(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].password.as_deref(), Some("s3cret"));

        delete(&conn, &saved.id).unwrap();
        assert!(list_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn update_keeps_password_when_blank() {
        let conn = test_db();
        let saved = save(
            &conn,
            SaveConnectionInput {
                id: None,
                name: "pg".into(),
                engine: "postgres".into(),
                color_id: "teal".into(),
                environment: "local".into(),
                read_only: true,
                notes: None,
                plugin_config: json!({"host": "127.0.0.1", "database": "postgres"}),
                password: Some("keep-me".into()),
                secret_ref: None,
                clear_secret: false,
            },
        )
        .unwrap();

        let updated = save(
            &conn,
            SaveConnectionInput {
                id: Some(saved.id.clone()),
                name: "pg-renamed".into(),
                engine: "postgres".into(),
                color_id: "teal".into(),
                environment: "local".into(),
                read_only: true,
                notes: None,
                plugin_config: json!({"host": "127.0.0.1", "database": "postgres"}),
                password: None,
                secret_ref: None,
                clear_secret: false,
            },
        )
        .unwrap();

        assert_eq!(updated.name, "pg-renamed");
        assert_eq!(updated.password.as_deref(), Some("keep-me"));
        assert!(updated.password_set);
    }

    #[test]
    fn clear_secret_removes_password() {
        let conn = test_db();
        let saved = save(
            &conn,
            SaveConnectionInput {
                id: None,
                name: "pg".into(),
                engine: "postgres".into(),
                color_id: "teal".into(),
                environment: "local".into(),
                read_only: true,
                notes: None,
                plugin_config: json!({}),
                password: Some("x".into()),
                secret_ref: None,
                clear_secret: false,
            },
        )
        .unwrap();

        let cleared = save(
            &conn,
            SaveConnectionInput {
                id: Some(saved.id),
                name: "pg".into(),
                engine: "postgres".into(),
                color_id: "teal".into(),
                environment: "local".into(),
                read_only: true,
                notes: None,
                plugin_config: json!({}),
                password: None,
                secret_ref: None,
                clear_secret: true,
            },
        )
        .unwrap();

        assert!(!cleared.password_set);
        assert!(cleared.password.is_none());
    }

    #[test]
    fn ignores_legacy_placeholder_secret_ref() {
        let conn = test_db();
        let saved = save(
            &conn,
            SaveConnectionInput {
                id: None,
                name: "pg".into(),
                engine: "postgres".into(),
                color_id: "teal".into(),
                environment: "local".into(),
                read_only: true,
                notes: None,
                plugin_config: json!({}),
                password: None,
                secret_ref: Some("mystique-db/connection/abc".into()),
                clear_secret: false,
            },
        )
        .unwrap();
        assert!(!saved.password_set);
        assert!(saved.password.is_none());
    }
}
