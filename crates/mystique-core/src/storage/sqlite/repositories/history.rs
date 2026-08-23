use rusqlite::{params, Connection};

use crate::domain::{now_iso, AppendHistoryInput, QueryHistoryItem};
use crate::error::AppResult;
use crate::storage::sqlite::repositories::ids::{id_string, id_string_opt, parse_id, parse_id_opt};

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueryHistoryItem> {
    let success_i: i64 = row.get("success")?;
    Ok(QueryHistoryItem {
        id: id_string(row, "id")?,
        connection_id: id_string_opt(row, "connection_id")?,
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
    let ran_at = now_iso();
    let language = input
        .language
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let connection_id_n = parse_id_opt(input.connection_id.as_deref())?;

    conn.execute(
        "
        INSERT INTO query_history (
          connection_id, engine, body, language, success,
          row_count, duration_ms, error_message, ran_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ",
        params![
            connection_id_n,
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
        id: conn.last_insert_rowid().to_string(),
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
    let connection_id = parse_id(connection_id)?;
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
    let connection_id = parse_id(connection_id)?;
    let n = conn.execute(
        "DELETE FROM query_history WHERE connection_id = ?1",
        params![connection_id],
    )?;
    Ok(n)
}
