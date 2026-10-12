use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::now_iso;
use crate::domain::{SaveQueryInput, SavedQuery};
use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repositories::ids::{id_string, parse_id};

fn map_query(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedQuery> {
    Ok(SavedQuery {
        id: id_string(row, "id")?,
        connection_id: id_string(row, "connection_id")?,
        name: row.get("name")?,
        body: row.get("body")?,
        language: row.get("language")?,
        sort_order: row.get("sort_order")?,
        favorite: row.get::<_, i64>("favorite")? != 0,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn ensure_connection_exists(conn: &Connection, connection_id: &str) -> AppResult<()> {
    let connection_id = parse_id(connection_id)?;
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

pub fn list_all_queries(conn: &Connection) -> AppResult<Vec<SavedQuery>> {
    let mut stmt = conn.prepare(
        "
        SELECT id, connection_id, name, body, language, sort_order, favorite, created_at, updated_at
        FROM saved_queries
        ORDER BY connection_id, favorite DESC, sort_order, name COLLATE NOCASE
        ",
    )?;
    let rows = stmt.query_map([], map_query)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn list_queries(conn: &Connection, connection_id: &str) -> AppResult<Vec<SavedQuery>> {
    let connection_id = parse_id(connection_id)?;
    let mut stmt = conn.prepare(
        "
        SELECT id, connection_id, name, body, language, sort_order, favorite, created_at, updated_at
        FROM saved_queries
        WHERE connection_id = ?1
        ORDER BY favorite DESC, sort_order, name COLLATE NOCASE
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
    let id_n = parse_id(id)?;
    conn.query_row(
        "
        SELECT id, connection_id, name, body, language, sort_order, favorite, created_at, updated_at
        FROM saved_queries WHERE id = ?1
        ",
        params![id_n],
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

    let connection_id = parse_id(&input.connection_id)?;
    if let Some(id) = input.id.as_ref().filter(|s| !s.is_empty()) {
        let id_n = parse_id(id)?;
        let favorite: Option<i64> = input.favorite.map(|f| i64::from(f));
        let n = conn.execute(
            "
            UPDATE saved_queries SET
              name = ?1,
              body = ?2,
              language = ?3,
              sort_order = ?4,
              favorite = COALESCE(?5, favorite),
              updated_at = ?6
            WHERE id = ?7 AND connection_id = ?8
            ",
            params![
                input.name.trim(),
                input.body,
                language,
                input.sort_order,
                favorite,
                now,
                id_n,
                connection_id,
            ],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("saved query '{id}'")));
        }
        get_query(conn, id)
    } else {
        let favorite = i64::from(input.favorite.unwrap_or(false));
        conn.execute(
            "
            INSERT INTO saved_queries (
              connection_id, name, body, language, sort_order, favorite, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                connection_id,
                input.name.trim(),
                input.body,
                language,
                input.sort_order,
                favorite,
                now,
                now,
            ],
        )?;
        get_query(conn, &conn.last_insert_rowid().to_string())
    }
}

pub fn delete_query(conn: &Connection, id: &str) -> AppResult<()> {
    let id_n = parse_id(id)?;
    let n = conn.execute("DELETE FROM saved_queries WHERE id = ?1", params![id_n])?;
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
                copy_secret_from: None,
                favorite: None,
                ssh_config: crate::domain::SshConfig::default(),
                ssh_password: None,
                clear_ssh_secret: false,
                tls_key_passphrase: None,
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
                favorite: None,
            },
        )
        .unwrap();
        assert!(saved.id.parse::<i64>().is_ok(), "id should be numeric");
        assert_eq!(saved.name, "Active users");
        assert_eq!(saved.body, "SELECT 1");
        assert!(!saved.favorite);

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
                favorite: Some(true),
            },
        )
        .unwrap();
        assert_eq!(updated.id, saved.id);
        assert_eq!(updated.name, "Active users v2");
        assert!(updated.favorite);
        assert_eq!(list_queries(&conn, &profile.id).unwrap().len(), 1);
    }

    fn save_profile(conn: &Connection, name: &str) -> crate::domain::ConnectionProfile {
        connections::save(
            conn,
            SaveConnectionInput {
                id: None,
                name: name.into(),
                engine: "postgres".into(),
                color_id: "slate".into(),
                environment: "local".into(),
                read_only: false,
                notes: None,
                plugin_config: json!({}),
                password: None,
                secret_ref: None,
                clear_secret: false,
                copy_secret_from: None,
                favorite: None,
                ssh_config: crate::domain::SshConfig::default(),
                ssh_password: None,
                clear_ssh_secret: false,
                tls_key_passphrase: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn update_does_not_cross_connections() {
        let conn = mem();
        let scratch = save_profile(&conn, "scratch");
        let prod = save_profile(&conn, "prod");
        let saved = save_query(
            &conn,
            SaveQueryInput {
                id: None,
                connection_id: prod.id.clone(),
                name: "prod query".into(),
                body: "SELECT secret".into(),
                language: Some("sql".into()),
                sort_order: 0,
                favorite: None,
            },
        )
        .unwrap();

        let err = save_query(
            &conn,
            SaveQueryInput {
                id: Some(saved.id.clone()),
                connection_id: scratch.id,
                name: "hijack".into(),
                body: "SELECT 1".into(),
                language: Some("sql".into()),
                sort_order: 0,
                favorite: None,
            },
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "cross-connection update must miss the row, got {err}"
        );
        let still = get_query(&conn, &saved.id).unwrap();
        assert_eq!(still.body, "SELECT secret");
        assert_eq!(still.connection_id, prod.id);
        assert_eq!(still.name, "prod query");
    }
}
