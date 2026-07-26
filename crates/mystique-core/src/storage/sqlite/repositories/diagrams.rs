use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::domain::{now_iso, ErDiagram, SaveErDiagramInput};
use crate::error::{AppError, AppResult};

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ErDiagram> {
    let body_str: String = row.get(3)?;
    let body: JsonValue = serde_json::from_str(&body_str).unwrap_or(JsonValue::Object(Default::default()));
    Ok(ErDiagram {
        id: row.get(0)?,
        name: row.get(1)?,
        family: row.get(2)?,
        body,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

pub fn list_all(conn: &Connection, family: Option<&str>) -> AppResult<Vec<ErDiagram>> {
    let mut out = Vec::new();
    if let Some(fam) = family {
        let mut stmt = conn.prepare(
            "SELECT id, name, family, body, created_at, updated_at
             FROM er_diagrams WHERE family = ?1
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map(params![fam], map_row)?;
        for r in rows {
            out.push(r?);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, name, family, body, created_at, updated_at
             FROM er_diagrams ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], map_row)?;
        for r in rows {
            out.push(r?);
        }
    }
    Ok(out)
}

pub fn get_by_id(conn: &Connection, id: &str) -> AppResult<ErDiagram> {
    conn.query_row(
        "SELECT id, name, family, body, created_at, updated_at
         FROM er_diagrams WHERE id = ?1",
        params![id],
        map_row,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("diagram {id}")))
}

pub fn save(conn: &Connection, input: SaveErDiagramInput) -> AppResult<ErDiagram> {
    let family = input.family.trim().to_lowercase();
    if !matches!(family.as_str(), "sql" | "nosql" | "redis") {
        return Err(AppError::Invalid(format!(
            "family must be sql|nosql|redis, got {family}"
        )));
    }
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Invalid("diagram name required".into()));
    }
    let body = serde_json::to_string(&input.body)
        .map_err(|e| AppError::Invalid(format!("body json: {e}")))?;
    let now = now_iso();

    let id = input
        .id
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let existing: Option<String> = conn
        .query_row(
            "SELECT created_at FROM er_diagrams WHERE id = ?1",
            params![&id],
            |r| r.get(0),
        )
        .optional()?;

    if let Some(created_at) = existing {
        conn.execute(
            "UPDATE er_diagrams SET name = ?1, family = ?2, body = ?3, updated_at = ?4 WHERE id = ?5",
            params![name, family, body, now, id],
        )?;
        Ok(ErDiagram {
            id,
            name: name.to_string(),
            family,
            body: input.body,
            created_at,
            updated_at: now,
        })
    } else {
        conn.execute(
            "INSERT INTO er_diagrams (id, name, family, body, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![&id, name, &family, body, &now, &now],
        )?;
        Ok(ErDiagram {
            id,
            name: name.to_string(),
            family,
            body: input.body,
            created_at: now.clone(),
            updated_at: now,
        })
    }
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM er_diagrams WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(AppError::NotFound(format!("diagram {id}")));
    }
    Ok(())
}
