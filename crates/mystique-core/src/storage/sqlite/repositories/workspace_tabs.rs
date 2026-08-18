use rusqlite::{params, Connection};

use crate::domain::now_iso;
use crate::domain::{ReplaceWorkspaceTabsInput, SaveWorkspaceTabInput, WorkspaceTabRecord};
use crate::error::{AppError, AppResult};

fn map_tab(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceTabRecord> {
    let is_active: i64 = row.get("is_active")?;
    let write_enabled: i64 = row.get("write_enabled")?;
    let is_dirty: i64 = row.get("is_dirty")?;
    Ok(WorkspaceTabRecord {
        id: row.get("id")?,
        connection_id: row.get("connection_id")?,
        kind: row.get("kind")?,
        title: row.get("title")?,
        sort_order: row.get("sort_order")?,
        is_active: is_active != 0,
        saved_query_id: row.get("saved_query_id")?,
        body: row.get("body")?,
        language: row.get("language")?,
        write_enabled: write_enabled != 0,
        table_id: row.get("table_id")?,
        model_database: row.get("model_database")?,
        is_dirty: is_dirty != 0,
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
        return Err(AppError::NotFound(format!("connection '{connection_id}'")));
    }
    Ok(())
}

fn normalize_kind(kind: &str) -> AppResult<&'static str> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "query" => Ok("query"),
        "table" => Ok("table"),
        "model" => Ok("model"),
        other => Err(AppError::Invalid(format!(
            "invalid tab kind '{other}' (expected query|table|model)"
        ))),
    }
}

pub fn list_tabs(conn: &Connection, connection_id: &str) -> AppResult<Vec<WorkspaceTabRecord>> {
    let mut stmt = conn.prepare(
        "
        SELECT id, connection_id, kind, title, sort_order, is_active,
               saved_query_id, body, language, write_enabled,
               table_id, model_database, is_dirty, created_at, updated_at
        FROM workspace_tabs
        WHERE connection_id = ?1
        ORDER BY sort_order, created_at
        ",
    )?;
    let rows = stmt.query_map(params![connection_id], map_tab)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn upsert_tab(conn: &Connection, input: &SaveWorkspaceTabInput, now: &str) -> AppResult<()> {
    let kind = normalize_kind(&input.kind)?;
    if input.id.trim().is_empty() {
        return Err(AppError::Invalid("tab id is required".into()));
    }
    if input.title.trim().is_empty() {
        return Err(AppError::Invalid("tab title is required".into()));
    }
    let saved_query_id = input
        .saved_query_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let language = input
        .language
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let table_id = input
        .table_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let model_database = input
        .model_database
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    conn.execute(
        "
        INSERT INTO workspace_tabs (
          id, connection_id, kind, title, sort_order, is_active,
          saved_query_id, body, language, write_enabled,
          table_id, model_database, is_dirty, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)
        ON CONFLICT(id) DO UPDATE SET
          connection_id = excluded.connection_id,
          kind = excluded.kind,
          title = excluded.title,
          sort_order = excluded.sort_order,
          is_active = excluded.is_active,
          saved_query_id = excluded.saved_query_id,
          body = excluded.body,
          language = excluded.language,
          write_enabled = excluded.write_enabled,
          table_id = excluded.table_id,
          model_database = excluded.model_database,
          is_dirty = excluded.is_dirty,
          updated_at = excluded.updated_at
        ",
        params![
            input.id.trim(),
            input.connection_id,
            kind,
            input.title.trim(),
            input.sort_order,
            if input.is_active { 1 } else { 0 },
            saved_query_id,
            input.body,
            language,
            if input.write_enabled { 1 } else { 0 },
            table_id,
            model_database,
            if input.is_dirty { 1 } else { 0 },
            now,
        ],
    )?;
    Ok(())
}

/// Replace all tabs for a connection with the provided list (atomic).
pub fn replace_tabs(
    conn: &Connection,
    input: ReplaceWorkspaceTabsInput,
) -> AppResult<Vec<WorkspaceTabRecord>> {
    ensure_connection_exists(conn, &input.connection_id)?;
    let now = now_iso();
    let active_id = input
        .active_tab_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM workspace_tabs WHERE connection_id = ?1",
        params![input.connection_id],
    )?;

    for (i, mut tab) in input.tabs.into_iter().enumerate() {
        tab.connection_id = input.connection_id.clone();
        tab.sort_order = i as i64;
        if let Some(ref aid) = active_id {
            tab.is_active = tab.id == *aid;
        }
        upsert_tab(&tx, &tab, &now)?;
    }
    tx.commit()?;

    list_tabs(conn, &input.connection_id)
}

pub fn delete_tab(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM workspace_tabs WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(AppError::NotFound(format!("workspace tab '{id}'")));
    }
    Ok(())
}

pub fn delete_tabs_for_connection(conn: &Connection, connection_id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM workspace_tabs WHERE connection_id = ?1",
        params![connection_id],
    )?;
    Ok(())
}
