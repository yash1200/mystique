use std::collections::BTreeMap;

use rusqlite::{params, Connection};

use crate::domain::{now_iso, GridColumnLayoutRecord, GridSaveRecord, GridSaveSummary};
use crate::error::{AppError, AppResult};

const HISTORY_KEEP: i64 = 20;

pub fn get_layout(
    conn: &Connection,
    connection_id: &str,
    table_id: &str,
) -> AppResult<Option<GridColumnLayoutRecord>> {
    let mut stmt = conn.prepare(
        "SELECT column_order, hidden, frozen, widths FROM grid_column_layout
         WHERE connection_id = ?1 AND table_id = ?2",
    )?;
    let mut rows = stmt.query(params![connection_id, table_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let widths: String = row.get("widths")?;
    Ok(Some(GridColumnLayoutRecord {
        connection_id: connection_id.to_string(),
        table_id: table_id.to_string(),
        order: parse_names(row.get("column_order")?)?,
        hidden: parse_names(row.get("hidden")?)?,
        frozen: parse_names(row.get("frozen")?)?,
        widths: serde_json::from_str(&widths).map_err(|err| AppError::Invalid(err.to_string()))?,
    }))
}

/// Widths outside this range are dropped on save; the grid then auto-sizes.
const WIDTH_RANGE: std::ops::RangeInclusive<u32> = 24..=4000;

pub fn save_layout(conn: &Connection, layout: &GridColumnLayoutRecord) -> AppResult<()> {
    require_ids(&layout.connection_id, &layout.table_id)?;
    let widths: BTreeMap<&String, &u32> = layout
        .widths
        .iter()
        .filter(|(name, width)| !name.is_empty() && WIDTH_RANGE.contains(width))
        .collect();
    conn.execute(
        "INSERT INTO grid_column_layout
           (connection_id, table_id, column_order, hidden, frozen, widths, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(connection_id, table_id) DO UPDATE SET
           column_order = excluded.column_order,
           hidden = excluded.hidden,
           frozen = excluded.frozen,
           widths = excluded.widths,
           updated_at = excluded.updated_at",
        params![
            layout.connection_id,
            layout.table_id,
            serde_json::to_string(&layout.order)?,
            serde_json::to_string(&layout.hidden)?,
            serde_json::to_string(&layout.frozen)?,
            serde_json::to_string(&widths)?,
            now_iso(),
        ],
    )?;
    Ok(())
}

pub fn put_save(conn: &Connection, save: &GridSaveRecord) -> AppResult<()> {
    require_ids(&save.connection_id, &save.table_key)?;
    if save.undo.inverse.is_empty() {
        return Err(AppError::Invalid(
            "A rollback needs at least one statement.".into(),
        ));
    }
    conn.execute(
        "INSERT INTO grid_saves (save_id, connection_id, table_key, undo, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            save.save_id,
            save.connection_id,
            save.table_key,
            serde_json::to_string(&save.undo)?,
            save.created_at,
        ],
    )?;
    conn.execute(
        "DELETE FROM grid_saves
         WHERE connection_id = ?1 AND table_key = ?2
           AND rowid NOT IN (
             SELECT rowid FROM grid_saves
             WHERE connection_id = ?1 AND table_key = ?2
             ORDER BY rowid DESC
             LIMIT ?3
           )",
        params![save.connection_id, save.table_key, HISTORY_KEEP],
    )?;
    Ok(())
}

pub fn get_save(conn: &Connection, save_id: &str) -> AppResult<Option<GridSaveRecord>> {
    let mut stmt = conn.prepare(
        "SELECT save_id, connection_id, table_key, undo, created_at FROM grid_saves
         WHERE save_id = ?1",
    )?;
    let mut rows = stmt.query(params![save_id])?;
    match rows.next()? {
        Some(row) => Ok(Some(save_row(row)?)),
        None => Ok(None),
    }
}

/// Deletes the save and returns it. A second call for the same id gets `None`.
pub fn take_save(conn: &Connection, save_id: &str) -> AppResult<Option<GridSaveRecord>> {
    let mut stmt = conn.prepare(
        "DELETE FROM grid_saves WHERE save_id = ?1
         RETURNING save_id, connection_id, table_key, undo, created_at",
    )?;
    let mut rows = stmt.query(params![save_id])?;
    match rows.next()? {
        Some(row) => Ok(Some(save_row(row)?)),
        None => Ok(None),
    }
}

pub fn latest_save(
    conn: &Connection,
    connection_id: &str,
    table_key: &str,
) -> AppResult<Option<GridSaveSummary>> {
    let mut stmt = conn.prepare(
        "SELECT save_id, created_at FROM grid_saves
         WHERE connection_id = ?1 AND table_key = ?2
         ORDER BY rowid DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![connection_id, table_key])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    Ok(Some(GridSaveSummary {
        save_id: row.get("save_id")?,
        created_at: row.get("created_at")?,
    }))
}

fn save_row(row: &rusqlite::Row<'_>) -> AppResult<GridSaveRecord> {
    let undo: String = row.get("undo")?;
    Ok(GridSaveRecord {
        save_id: row.get("save_id")?,
        connection_id: row.get("connection_id")?,
        table_key: row.get("table_key")?,
        undo: serde_json::from_str(&undo).map_err(|err| AppError::Invalid(err.to_string()))?,
        created_at: row.get("created_at")?,
    })
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

#[cfg(test)]
mod tests {
    use mystique_connector_api::{GridUndo, RowCheck};

    use super::*;
    use crate::storage::sqlite::migrate::migrate;

    fn record(save_id: &str) -> GridSaveRecord {
        GridSaveRecord {
            save_id: save_id.into(),
            connection_id: "c1".into(),
            table_key: "t".into(),
            undo: GridUndo {
                checks: vec![RowCheck {
                    sql: "SELECT * FROM t WHERE id = ?".into(),
                    params: vec![serde_json::json!(1)],
                    hash: "abc".into(),
                }],
                inverse: vec!["UPDATE t SET n = 1 WHERE id = 1".into()],
            },
            created_at: "2026-01-01T00:00:00.000Z".into(),
        }
    }

    fn layout(widths: &[(&str, u32)]) -> GridColumnLayoutRecord {
        GridColumnLayoutRecord {
            connection_id: "c1".into(),
            table_id: "db.public.users".into(),
            order: vec!["name".into(), "id".into()],
            hidden: vec!["secret".into()],
            frozen: vec!["id".into()],
            widths: widths
                .iter()
                .map(|(name, w)| (name.to_string(), *w))
                .collect(),
        }
    }

    #[test]
    fn column_widths_round_trip_per_table() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let saved = layout(&[("id", 64), ("name", 240)]);
        save_layout(&conn, &saved).unwrap();
        assert_eq!(
            get_layout(&conn, "c1", "db.public.users").unwrap(),
            Some(saved)
        );
        // Same table name in another schema is a separate layout.
        assert!(get_layout(&conn, "c1", "db.audit.users").unwrap().is_none());

        let resized = layout(&[("name", 300)]);
        save_layout(&conn, &resized).unwrap();
        assert_eq!(
            get_layout(&conn, "c1", "db.public.users").unwrap(),
            Some(resized)
        );
    }

    #[test]
    fn out_of_range_widths_are_dropped() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        save_layout(
            &conn,
            &layout(&[("id", 0), ("name", 9000), ("", 80), ("ok", 90)]),
        )
        .unwrap();
        let back = get_layout(&conn, "c1", "db.public.users").unwrap().unwrap();
        assert_eq!(back.widths, layout(&[("ok", 90)]).widths);
    }

    #[test]
    fn a_layout_without_widths_still_deserializes() {
        let json = r#"{"connectionId":"c1","tableId":"t","order":[],"hidden":[],"frozen":[]}"#;
        let record: GridColumnLayoutRecord = serde_json::from_str(json).unwrap();
        assert!(record.widths.is_empty());
    }

    #[test]
    fn a_save_can_be_taken_once() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        put_save(&conn, &record("s1")).unwrap();
        assert_eq!(get_save(&conn, "s1").unwrap(), Some(record("s1")));
        assert_eq!(take_save(&conn, "s1").unwrap(), Some(record("s1")));
        assert!(take_save(&conn, "s1").unwrap().is_none());
        assert!(get_save(&conn, "s1").unwrap().is_none());
    }

    #[test]
    fn latest_save_is_the_newest_one_left() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        put_save(&conn, &record("s1")).unwrap();
        put_save(&conn, &record("s2")).unwrap();
        let latest = latest_save(&conn, "c1", "t").unwrap().map(|s| s.save_id);
        assert_eq!(latest.as_deref(), Some("s2"));
        take_save(&conn, "s2").unwrap();
        let latest = latest_save(&conn, "c1", "t").unwrap().map(|s| s.save_id);
        assert_eq!(latest.as_deref(), Some("s1"));
        assert!(latest_save(&conn, "c2", "t").unwrap().is_none());
    }
}
