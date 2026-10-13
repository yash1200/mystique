use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::domain::now_iso;
use crate::domain::{
    is_placeholder_secret_ref, validate_environment, ConnectionProfile, SaveConnectionInput,
};
use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repositories::ids::{id_string, parse_id};

fn password_set_from_stored(secret_ref: Option<&str>) -> bool {
    secret_ref.is_some_and(|s| {
        let t = s.trim();
        !t.is_empty() && !is_placeholder_secret_ref(t)
    })
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConnectionProfile> {
    let plugin_config_raw: String = row.get("plugin_config")?;
    let plugin_config: JsonValue =
        serde_json::from_str(&plugin_config_raw).unwrap_or(JsonValue::Object(Default::default()));
    let stored: Option<String> = row.get("secret_ref")?;
    let password_set = password_set_from_stored(stored.as_deref());
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
        password: None,
        ssh: {
            let raw: String = row.get("ssh_config")?;
            serde_json::from_str(&raw).unwrap_or_default()
        },
        ssh_password_set: password_set_from_stored(
            row.get::<_, Option<String>>("ssh_secret_ref")?.as_deref(),
        ),
        favorite: row.get::<_, i64>("favorite")? != 0,
        last_connected_at: row.get("last_connected_at")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

const SELECT_COLS: &str = "
  id, name, engine, color_id, environment, read_only, notes,
  plugin_config, secret_ref, ssh_config, ssh_secret_ref, favorite,
  last_connected_at, created_at, updated_at
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

/// Create or update a connection. Never writes a password into `secret_ref`;
/// [`ProfileService`] stores a keychain pointer after save.
pub fn save(conn: &Connection, input: SaveConnectionInput) -> AppResult<ConnectionProfile> {
    if input.name.trim().is_empty() {
        return Err(AppError::Invalid("name is required".into()));
    }
    if input.engine.trim().is_empty() {
        return Err(AppError::Invalid("engine is required".into()));
    }
    validate_environment(&input.environment).map_err(AppError::Invalid)?;

    let plugin_config = serde_json::to_string(&input.plugin_config)?;
    let ssh_config = serde_json::to_string(&input.ssh_config)?;
    let now = now_iso();
    let notes = input
        .notes
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    if let Some(id) = input.id.as_ref().filter(|s| !s.is_empty()) {
        let id_n = parse_id(id)?;
        let favorite: Option<i64> = input.favorite.map(|f| i64::from(f));
        let n = if input.clear_secret {
            conn.execute(
                "
                UPDATE connections SET
                  name = ?1,
                  engine = ?2,
                  color_id = ?3,
                  environment = ?4,
                  read_only = ?5,
                  notes = ?6,
                  plugin_config = ?7,
                  ssh_config = ?8,
                  secret_ref = NULL,
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
                    ssh_config,
                    favorite,
                    now,
                    id_n,
                ],
            )?
        } else {
            conn.execute(
                "
                UPDATE connections SET
                  name = ?1,
                  engine = ?2,
                  color_id = ?3,
                  environment = ?4,
                  read_only = ?5,
                  notes = ?6,
                  plugin_config = ?7,
                  ssh_config = ?8,
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
                    ssh_config,
                    favorite,
                    now,
                    id_n,
                ],
            )?
        };
        if n == 0 {
            return Err(AppError::NotFound(format!("connection '{id}'")));
        }
        if input.clear_ssh_secret {
            conn.execute(
                "UPDATE connections SET ssh_secret_ref = NULL WHERE id = ?1",
                params![id_n],
            )?;
        }
        get_by_id(conn, id)
    } else {
        let favorite = i64::from(input.favorite.unwrap_or(false));
        conn.execute(
            "
            INSERT INTO connections (
              name, engine, color_id, environment, read_only, notes,
              plugin_config, secret_ref, ssh_config, ssh_secret_ref, favorite,
              last_connected_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, NULL, ?9, NULL, ?10, ?11)
            ",
            params![
                input.name.trim(),
                input.engine.trim(),
                input.color_id.trim(),
                input.environment.trim(),
                if input.read_only { 1 } else { 0 },
                notes,
                plugin_config,
                ssh_config,
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

pub fn get_secret_ref(conn: &Connection, id: &str) -> AppResult<Option<String>> {
    let id_n = parse_id(id)?;
    let stored: Option<String> = conn
        .query_row(
            "SELECT secret_ref FROM connections WHERE id = ?1",
            params![id_n],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("connection '{id}'")))?;
    Ok(nonempty_ref(stored))
}

fn nonempty_ref(stored: Option<String>) -> Option<String> {
    stored.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    })
}

pub fn set_ssh_secret_ref(conn: &Connection, id: &str, secret_ref: Option<&str>) -> AppResult<()> {
    let id_n = parse_id(id)?;
    let n = conn.execute(
        "UPDATE connections SET ssh_secret_ref = ?1, updated_at = ?2 WHERE id = ?3",
        params![secret_ref, now_iso(), id_n],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("connection '{id}'")));
    }
    Ok(())
}

pub fn get_ssh_secret_ref(conn: &Connection, id: &str) -> AppResult<Option<String>> {
    let id_n = parse_id(id)?;
    let stored: Option<String> = conn
        .query_row(
            "SELECT ssh_secret_ref FROM connections WHERE id = ?1",
            params![id_n],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("connection '{id}'")))?;
    Ok(nonempty_ref(stored))
}

pub fn set_plugin_config(conn: &Connection, id: &str, config: &JsonValue) -> AppResult<()> {
    let id_n = parse_id(id)?;
    let text = serde_json::to_string(config)?;
    let n = conn.execute(
        "UPDATE connections SET plugin_config = ?1, updated_at = ?2 WHERE id = ?3",
        params![text, now_iso(), id_n],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("connection '{id}'")));
    }
    Ok(())
}

pub fn set_secret_ref(conn: &Connection, id: &str, secret_ref: Option<&str>) -> AppResult<()> {
    let id_n = parse_id(id)?;
    let n = conn.execute(
        "UPDATE connections SET secret_ref = ?1, updated_at = ?2 WHERE id = ?3",
        params![secret_ref, now_iso(), id_n],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("connection '{id}'")));
    }
    Ok(())
}

/// `(id, secret_ref)` for plaintext → keychain migration.
pub fn list_secret_refs(conn: &Connection) -> AppResult<Vec<(String, Option<String>)>> {
    let mut stmt = conn.prepare("SELECT id, secret_ref FROM connections")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            id_string(row, "id")?,
            row.get::<_, Option<String>>("secret_ref")?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
