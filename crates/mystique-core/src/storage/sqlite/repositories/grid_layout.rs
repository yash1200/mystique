use rusqlite::{params, Connection};

use crate::domain::{
    now_iso, AppendGridEditHistoryInput, GridColumnLayoutRecord, GridEditHistoryRecord,
};
use crate::error::{AppError, AppResult};

const HISTORY_KEEP: i64 = 20;

pub fn get_layout(
    conn: &Connection,
    connection_id: &str,
    table_id: &str,
) -> AppResult<Option<GridColumnLayoutRecord>> {
    let mut stmt = conn.prepare(
        "SELECT column_order, hidden, frozen FROM grid_column_layout
         WHERE connection_id = ?1 AND table_id = ?2",
    )?;
    let mut rows = stmt.query(params![connection_id, table_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    Ok(Some(GridColumnLayoutRecord {
        connection_id: connection_id.to_string(),
        table_id: table_id.to_string(),
        order: parse_names(row.get("column_order")?)?,
        hidden: parse_names(row.get("hidden")?)?,
        frozen: parse_names(row.get("frozen")?)?,
    }))
}

pub fn save_layout(conn: &Connection, layout: &GridColumnLayoutRecord) -> AppResult<()> {
    require_ids(&layout.connection_id, &layout.table_id)?;
    conn.execute(
        "INSERT INTO grid_column_layout
           (connection_id, table_id, column_order, hidden, frozen, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(connection_id, table_id) DO UPDATE SET
           column_order = excluded.column_order,
           hidden = excluded.hidden,
           frozen = excluded.frozen,
           updated_at = excluded.updated_at",
        params![
            layout.connection_id,
            layout.table_id,
            serde_json::to_string(&layout.order)?,
            serde_json::to_string(&layout.hidden)?,
            serde_json::to_string(&layout.frozen)?,
            now_iso(),
        ],
    )?;
    Ok(())
}

pub fn latest_history(
    conn: &Connection,
    connection_id: &str,
    table_id: &str,
) -> AppResult<Option<GridEditHistoryRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, rollback_sql, created_at FROM grid_edit_history
         WHERE connection_id = ?1 AND table_id = ?2
         ORDER BY id DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![connection_id, table_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    Ok(Some(GridEditHistoryRecord {
        id: row.get("id")?,
        connection_id: connection_id.to_string(),
        table_id: table_id.to_string(),
        rollback_sql: parse_names(row.get("rollback_sql")?)?,
        created_at: row.get("created_at")?,
    }))
}

pub fn append_history(conn: &Connection, input: &AppendGridEditHistoryInput) -> AppResult<i64> {
    require_ids(&input.connection_id, &input.table_id)?;
    if input.rollback_sql.is_empty() {
        return Err(AppError::Invalid(
            "Rollback history needs at least one statement.".into(),
        ));
    }
    conn.execute(
        "INSERT INTO grid_edit_history (connection_id, table_id, rollback_sql, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            input.connection_id,
            input.table_id,
            serde_json::to_string(&input.rollback_sql)?,
            now_iso(),
        ],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "DELETE FROM grid_edit_history
         WHERE connection_id = ?1 AND table_id = ?2
           AND id NOT IN (
             SELECT id FROM grid_edit_history
             WHERE connection_id = ?1 AND table_id = ?2
             ORDER BY id DESC
             LIMIT ?3
           )",
        params![input.connection_id, input.table_id, HISTORY_KEEP],
    )?;
    Ok(id)
}

fn parse_names(text: String) -> AppResult<Vec<String>> {
    serde_json::from_str(&text).map_err(|err| AppError::Invalid(err.to_string()))
}

fn require_ids(connection_id: &str, table_id: &str) -> AppResult<()> {
    if connection_id.trim().is_empty() || table_id.trim().is_empty() {
        return Err(AppError::Invalid(
            "Connection and table are required.".into(),
        ));
    }
    Ok(())
}
