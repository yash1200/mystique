use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::now_iso;
use crate::domain::{SaveQueryInput, SavedQuery};
use crate::error::{AppError, AppResult};

fn map_query(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedQuery> {
    Ok(SavedQuery {
        id: row.get("id")?,
        connection_id: row.get("connection_id")?,
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

pub fn list_queries(conn: &Connection, connection_id: &str) -> AppResult<Vec<SavedQuery>> {
    let mut stmt = conn.prepare(
        "
        SELECT id, connection_id, name, body, language, sort_order, created_at, updated_at
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
        SELECT id, connection_id, name, body, language, sort_order, created_at, updated_at
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
    let now = now_iso();
    let language = input
        .language
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    if let Some(id) = input.id.as_ref().filter(|s| !s.is_empty()) {
        let n = conn.execute(
            "
            UPDATE saved_queries SET
              name = ?1,
              body = ?2,
              language = ?3,
              sort_order = ?4,
              updated_at = ?5
            WHERE id = ?6
            ",
            params![
                input.name.trim(),
                input.body,
                language,
                input.sort_order,
                now,
                id,
            ],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("saved query '{id}'")));
        }
        get_query(conn, id)
    } else {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "
            INSERT INTO saved_queries (
              id, connection_id, name, body, language, sort_order, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                id,
                input.connection_id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SaveConnectionInput;
    use crate::storage::sqlite::migrate;
    use crate::storage::sqlite::repo::connections;
    use serde_json::json;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn save_and_list_query_per_connection() {
        let conn = mem();
        let profile = connections::save(
            &conn,
            SaveConnectionInput {
                id: None,
                name: "Dev".into(),
                engine: "postgres".into(),
                color_id: "slate".into(),
                environment: "local".into(),
                read_only: true,
                notes: None,
                plugin_config: json!({}),
                password: None,
                secret_ref: None,
                clear_secret: false,
            },
        )
        .unwrap();

        let saved = save_query(
            &conn,
            SaveQueryInput {
                id: None,
                connection_id: profile.id.clone(),
                name: "Active users".into(),
                body: "SELECT 1".into(),
                language: Some("sql".into()),
                sort_order: 0,
            },
        )
        .unwrap();
        assert!(!saved.id.is_empty());
        assert_eq!(saved.name, "Active users");
        assert_eq!(saved.body, "SELECT 1");

        let list = list_queries(&conn, &profile.id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, saved.id);

        let updated = save_query(
            &conn,
            SaveQueryInput {
                id: Some(saved.id.clone()),
                connection_id: profile.id.clone(),
                name: "Active users v2".into(),
                body: "SELECT 2".into(),
                language: Some("sql".into()),
                sort_order: 0,
            },
        )
        .unwrap();
        assert_eq!(updated.id, saved.id);
        assert_eq!(updated.name, "Active users v2");
        assert_eq!(list_queries(&conn, &profile.id).unwrap().len(), 1);
    }
}
