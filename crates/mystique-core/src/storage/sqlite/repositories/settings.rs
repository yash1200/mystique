use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::{now_iso, AppSetting};
use crate::error::AppResult;

pub fn get(conn: &Connection, key: &str) -> AppResult<Option<AppSetting>> {
    let row = conn
        .query_row(
            "SELECT key, value, updated_at FROM app_settings WHERE key = ?1",
            params![key],
            |r| {
                Ok(AppSetting {
                    key: r.get(0)?,
                    value: r.get(1)?,
                    updated_at: r.get(2)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> AppResult<AppSetting> {
    let now = now_iso();
    conn.execute(
        "
        INSERT INTO app_settings (key, value, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET
          value = excluded.value,
          updated_at = excluded.updated_at
        ",
        params![key, value, now],
    )?;
    Ok(AppSetting {
        key: key.to_string(),
        value: value.to_string(),
        updated_at: now,
    })
}

pub fn list_all(conn: &Connection) -> AppResult<Vec<AppSetting>> {
    let mut stmt = conn.prepare("SELECT key, value, updated_at FROM app_settings ORDER BY key")?;
    let rows = stmt.query_map([], |r| {
        Ok(AppSetting {
            key: r.get(0)?,
            value: r.get(1)?,
            updated_at: r.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn delete(conn: &Connection, key: &str) -> AppResult<bool> {
    let n = conn.execute("DELETE FROM app_settings WHERE key = ?1", params![key])?;
    Ok(n > 0)
}
