use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::domain::{now_iso, AppendHistoryInput, QueryHistoryItem};
use crate::error::AppResult;

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueryHistoryItem> {
    let success_i: i64 = row.get("success")?;
    Ok(QueryHistoryItem {
        id: row.get("id")?,
        connection_id: row.get("connection_id")?,
        engine: row.get("engine")?,
        body: row.get("body")?,
        language: row.get("language")?,
        success: success_i != 0,
        row_count: row.get("row_count")?,
        duration_ms: row.get("duration_ms")?,
        error_message: row.get("error_message")?,
        ran_at: row.get("ran_at")?,
    })
}

pub fn append(conn: &Connection, input: AppendHistoryInput) -> AppResult<QueryHistoryItem> {
    let id = Uuid::new_v4().to_string();
    let ran_at = now_iso();
    let language = input
        .language
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    conn.execute(
        "
        INSERT INTO query_history (
          id, connection_id, engine, body, language, success,
          row_count, duration_ms, error_message, ran_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ",
        params![
            id,
            input.connection_id,
            input.engine,
            input.body,
            language,
            if input.success { 1 } else { 0 },
            input.row_count,
            input.duration_ms,
            input.error_message,
            ran_at,
        ],
    )?;

    Ok(QueryHistoryItem {
        id,
        connection_id: input.connection_id,
        engine: input.engine,
        body: input.body,
        language,
        success: input.success,
        row_count: input.row_count,
        duration_ms: input.duration_ms,
        error_message: input.error_message,
        ran_at,
    })
}

pub fn list_for_connection(
    conn: &Connection,
    connection_id: &str,
    limit: i64,
) -> AppResult<Vec<QueryHistoryItem>> {
    let limit = limit.clamp(1, 500);
    let mut stmt = conn.prepare(
        "
        SELECT id, connection_id, engine, body, language, success,
               row_count, duration_ms, error_message, ran_at
        FROM query_history
        WHERE connection_id = ?1
        ORDER BY ran_at DESC
        LIMIT ?2
        ",
    )?;
    let rows = stmt.query_map(params![connection_id, limit], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn list_recent(conn: &Connection, limit: i64) -> AppResult<Vec<QueryHistoryItem>> {
    let limit = limit.clamp(1, 500);
    let mut stmt = conn.prepare(
        "
        SELECT id, connection_id, engine, body, language, success,
               row_count, duration_ms, error_message, ran_at
        FROM query_history
        ORDER BY ran_at DESC
        LIMIT ?1
        ",
    )?;
    let rows = stmt.query_map(params![limit], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn retain_latest(conn: &Connection, keep: i64) -> AppResult<usize> {
    let keep = keep.max(0);
    let n = conn.execute(
        "
        DELETE FROM query_history
        WHERE id NOT IN (
          SELECT id FROM query_history ORDER BY ran_at DESC LIMIT ?1
        )
        ",
        params![keep],
    )?;
    Ok(n)
}

pub fn clear_for_connection(conn: &Connection, connection_id: &str) -> AppResult<usize> {
    let n = conn.execute(
        "DELETE FROM query_history WHERE connection_id = ?1",
        params![connection_id],
    )?;
    Ok(n)
}
