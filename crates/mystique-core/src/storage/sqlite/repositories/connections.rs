use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::domain::now_iso;
use crate::domain::{
    is_placeholder_secret_ref, validate_environment, ConnectionProfile, SaveConnectionInput,
};
use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repositories::ids::{id_string, parse_id};

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
        id: id_string(row, "id")?,
        name: row.get("name")?,
        engine: row.get("engine")?,
        color_id: row.get("color_id")?,
        environment: row.get("environment")?,
        read_only: read_only_i != 0,
        notes: row.get("notes")?,
        plugin_config,
        password_set,
        password,
        favorite: row.get::<_, i64>("favorite")? != 0,
        last_connected_at: row.get("last_connected_at")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

const SELECT_COLS: &str = "
  id, name, engine, color_id, environment, read_only, notes,
  plugin_config, secret_ref, favorite, last_connected_at, created_at, updated_at
";

pub fn list_all(conn: &Connection) -> AppResult<Vec<ConnectionProfile>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS} FROM connections ORDER BY favorite DESC, name COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn get_by_id(conn: &Connection, id: &str) -> AppResult<ConnectionProfile> {
    let id_n = parse_id(id)?;
    conn.query_row(
        &format!("SELECT {SELECT_COLS} FROM connections WHERE id = ?1"),
        params![id_n],
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
        let id_n = parse_id(id)?;
        let prev: Option<String> = conn
            .query_row(
                "SELECT secret_ref FROM connections WHERE id = ?1",
                params![id_n],
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
        let id_n = parse_id(id)?;
        let secret_ref = resolve_password_to_store(conn, &input, Some(id))?;
        let favorite: Option<i64> = input.favorite.map(|f| i64::from(f));

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
              favorite = COALESCE(?9, favorite),
              updated_at = ?10
            WHERE id = ?11
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
                favorite,
                now,
                id_n,
            ],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("connection '{id}'")));
        }
        get_by_id(conn, id)
    } else {
        let secret_ref = resolve_password_to_store(conn, &input, None)?;
        let favorite = i64::from(input.favorite.unwrap_or(false));
        conn.execute(
            "
            INSERT INTO connections (
              name, engine, color_id, environment, read_only, notes,
              plugin_config, secret_ref, favorite, last_connected_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11)
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
                favorite,
                now,
                now,
            ],
        )?;
        get_by_id(conn, &conn.last_insert_rowid().to_string())
    }
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let id_n = parse_id(id)?;
    let n = conn.execute("DELETE FROM connections WHERE id = ?1", params![id_n])?;
    if n == 0 {
        return Err(AppError::NotFound(format!("connection '{id}'")));
    }
    Ok(())
}

pub fn touch_last_connected(conn: &Connection, id: &str) -> AppResult<ConnectionProfile> {
    let now = now_iso();
    let id_n = parse_id(id)?;
    let n = conn.execute(
        "UPDATE connections SET last_connected_at = ?1, updated_at = ?1 WHERE id = ?2",
        params![now, id_n],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("connection '{id}'")));
    }
    get_by_id(conn, id)
}

/// Returns the stored plaintext password (if any).
#[allow(dead_code)]
pub fn get_password(conn: &Connection, id: &str) -> AppResult<Option<String>> {
    let id_n = parse_id(id)?;
    let stored: Option<String> = conn
        .query_row(
            "SELECT secret_ref FROM connections WHERE id = ?1",
            params![id_n],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("connection '{id}'")))?;
    Ok(password_from_stored(stored))
}

#[cfg(test)]
mod tests;
