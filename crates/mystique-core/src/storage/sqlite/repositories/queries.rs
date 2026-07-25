use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::domain::now_iso;
use crate::domain::{
    QueryGroup, SaveQueryGroupInput, SaveQueryInput, SavedQuery,
};


fn map_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueryGroup> {
    Ok(QueryGroup {
        id: row.get("id")?,
        connection_id: row.get("connection_id")?,
        name: row.get("name")?,
        sort_order: row.get("sort_order")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_query(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedQuery> {
    Ok(SavedQuery {
        id: row.get("id")?,
        connection_id: row.get("connection_id")?,
        group_id: row.get("group_id")?,
        name: row.get("name")?,
        body: row.get("body")?,
        language: row.get("language")?,
        sort_order: row.get("sort_order")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn ensure_connection_exists(conn: &Connection, connection_id: &str) -> AppResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM connections WHERE id = ?1)",
        params![connection_id],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(AppError::NotFound(format!(
            "connection '{connection_id}'"
        )));
    }
    Ok(())
}

// —— Groups ——

pub fn list_groups(conn: &Connection, connection_id: &str) -> AppResult<Vec<QueryGroup>> {
    let mut stmt = conn.prepare(
        "
        SELECT id, connection_id, name, sort_order, created_at, updated_at
        FROM query_groups
        WHERE connection_id = ?1
        ORDER BY sort_order, name COLLATE NOCASE
        ",
    )?;
    let rows = stmt.query_map(params![connection_id], map_group)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn get_group(conn: &Connection, id: &str) -> AppResult<QueryGroup> {
    conn.query_row(
        "
        SELECT id, connection_id, name, sort_order, created_at, updated_at
        FROM query_groups WHERE id = ?1
        ",
        params![id],
        map_group,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("query group '{id}'")))
}

pub fn save_group(conn: &Connection, input: SaveQueryGroupInput) -> AppResult<QueryGroup> {
    if input.name.trim().is_empty() {
        return Err(AppError::Invalid("group name is required".into()));
    }
    ensure_connection_exists(conn, &input.connection_id)?;
    let now = now_iso();

    if let Some(id) = input.id.as_ref().filter(|s| !s.is_empty()) {
        conn.execute(
            "
            UPDATE query_groups SET
              name = ?1,
              sort_order = ?2,
              updated_at = ?3
            WHERE id = ?4
            ",
            params![input.name.trim(), input.sort_order, now, id],
        )?;
        get_group(conn, id)
    } else {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "
            INSERT INTO query_groups (id, connection_id, name, sort_order, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                id,
                input.connection_id,
                input.name.trim(),
                input.sort_order,
                now,
                now,
            ],
        )?;
        get_group(conn, &id)
    }
}

pub fn delete_group(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM query_groups WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(AppError::NotFound(format!("query group '{id}'")));
    }
    Ok(())
}

// —— Saved queries ——

pub fn list_queries(conn: &Connection, connection_id: &str) -> AppResult<Vec<SavedQuery>> {
    let mut stmt = conn.prepare(
        "
        SELECT id, connection_id, group_id, name, body, language, sort_order, created_at, updated_at
        FROM saved_queries
        WHERE connection_id = ?1
        ORDER BY sort_order, name COLLATE NOCASE
        ",
    )?;
    let rows = stmt.query_map(params![connection_id], map_query)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn get_query(conn: &Connection, id: &str) -> AppResult<SavedQuery> {
    conn.query_row(
        "
        SELECT id, connection_id, group_id, name, body, language, sort_order, created_at, updated_at
        FROM saved_queries WHERE id = ?1
        ",
        params![id],
        map_query,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("saved query '{id}'")))
}

pub fn save_query(conn: &Connection, input: SaveQueryInput) -> AppResult<SavedQuery> {
    if input.name.trim().is_empty() {
        return Err(AppError::Invalid("query name is required".into()));
    }
    ensure_connection_exists(conn, &input.connection_id)?;
    if let Some(gid) = input.group_id.as_ref().filter(|s| !s.is_empty()) {
        let ok: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM query_groups WHERE id = ?1)",
            params![gid],
            |r| r.get(0),
        )?;
        if !ok {
            return Err(AppError::NotFound(format!("query group '{gid}'")));
        }
    }
    let now = now_iso();
    let language = input
        .language
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let group_id = input
        .group_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    if let Some(id) = input.id.as_ref().filter(|s| !s.is_empty()) {
        conn.execute(
            "
            UPDATE saved_queries SET
              group_id = ?1,
              name = ?2,
              body = ?3,
              language = ?4,
              sort_order = ?5,
              updated_at = ?6
            WHERE id = ?7
            ",
            params![
                group_id,
                input.name.trim(),
                input.body,
                language,
                input.sort_order,
                now,
                id,
            ],
        )?;
        get_query(conn, id)
    } else {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "
            INSERT INTO saved_queries (
              id, connection_id, group_id, name, body, language, sort_order, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ",
            params![
                id,
                input.connection_id,
                group_id,
                input.name.trim(),
                input.body,
                language,
                input.sort_order,
                now,
                now,
            ],
        )?;
        get_query(conn, &id)
    }
}

pub fn delete_query(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM saved_queries WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(AppError::NotFound(format!("saved query '{id}'")));
    }
    Ok(())
}
