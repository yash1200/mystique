use rusqlite::{named_params, params, Connection};

use crate::domain::{
    now_iso, redact_history_text, AppendHistoryInput, HistoryList, QueryHistoryItem,
    QueryHistoryPage, QueryHistorySource, QUERY_HISTORY_CAP,
};
use crate::error::AppResult;
use crate::storage::sqlite::repositories::ids::{id_string, id_string_opt, parse_id_opt};

const BODY_MAX: usize = 1_048_576;
const ERROR_MAX: usize = 4_000;
const DATABASE_MAX: usize = 256;

const LIST_SQL: &str = "
SELECT id, connection_id, engine, body, language, success,
       row_count, duration_ms, error_message, ran_at,
       \"database\", affected_rows, source
FROM query_history
WHERE (:conn IS NULL OR connection_id = :conn)
  AND (:pat IS NULL OR body LIKE :pat ESCAPE '\\')
  AND (
    :ran IS NULL
    OR ran_at < :ran
    OR (ran_at = :ran AND id < :id)
  )
ORDER BY ran_at DESC, id DESC
LIMIT :lim
";

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueryHistoryItem> {
    let success_i: i64 = row.get("success")?;
    let source: String = row.get("source")?;
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
        database: row.get("database")?,
        affected_rows: row.get("affected_rows")?,
        source: QueryHistorySource::parse(&source),
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
    let connection_id = input
        .connection_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let connection_id_n = parse_id_opt(connection_id.as_deref())?;
    let body = clip_chars(&redact_history_text(&input.body), BODY_MAX);
    let error_message = input
        .error_message
        .as_deref()
        .map(redact_history_text)
        .map(|text| clip_chars(&text, ERROR_MAX));
    let database = stored_database(input.database.as_deref());
    let source = input.source.as_str();

    conn.execute_batch("BEGIN IMMEDIATE")?;
    let inserted = conn.execute(
        "
        INSERT INTO query_history (
          connection_id, engine, body, language, success,
          row_count, duration_ms, error_message, ran_at,
          \"database\", affected_rows, source
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ",
        params![
            connection_id_n,
            input.engine,
            body,
            language,
            if input.success { 1 } else { 0 },
            input.row_count,
            input.duration_ms,
            error_message.as_deref(),
            ran_at,
            database.as_deref(),
            input.affected_rows,
            source,
        ],
    );
    if let Err(err) = inserted {
        let _ = conn.execute_batch("ROLLBACK");
        return Err(err.into());
    }
    let id = conn.last_insert_rowid().to_string();
    if let Err(err) = retain_connection(conn, connection_id_n, QUERY_HISTORY_CAP) {
        let _ = conn.execute_batch("ROLLBACK");
        return Err(err);
    }
    if let Err(err) = conn.execute_batch("COMMIT") {
        let _ = conn.execute_batch("ROLLBACK");
        return Err(err.into());
    }
    Ok(QueryHistoryItem {
        id,
        connection_id,
        engine: input.engine,
        body,
        language,
        success: input.success,
        row_count: input.row_count,
        duration_ms: input.duration_ms,
        error_message,
        ran_at,
        database,
        affected_rows: input.affected_rows,
        source: input.source,
    })
}

/// `LIKE` contains-pattern. `%`, `_`, and `\` in the term match themselves.
pub fn like_contains_pattern(term: &str) -> String {
    let mut out = String::with_capacity(term.len() + 2);
    out.push('%');
    for ch in term.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
    out
}

pub fn list_page(conn: &Connection, query: &HistoryList) -> AppResult<QueryHistoryPage> {
    let limit = query.limit.clamp(1, 500);
    let fetch = limit + 1;
    let connection_id = parse_id_opt(query.connection_id.as_deref())?;
    let pattern = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(like_contains_pattern);
    let cursor_ran = query
        .cursor_ran_at
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let cursor_id = match cursor_ran.as_ref() {
        Some(_) => parse_id_opt(query.cursor_id.as_deref())?,
        None => None,
    };

    let mut stmt = conn.prepare(LIST_SQL)?;
    let rows = stmt.query_map(
        named_params! {
            ":conn": connection_id,
            ":pat": pattern,
            ":ran": cursor_ran,
            ":id": cursor_id,
            ":lim": fetch,
        },
        map_row,
    )?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    let has_more = items.len() > limit as usize;
    if has_more {
        items.truncate(limit as usize);
    }
    Ok(QueryHistoryPage { items, has_more })
}

pub fn list_for_connection(
    conn: &Connection,
    connection_id: &str,
    limit: i64,
) -> AppResult<Vec<QueryHistoryItem>> {
    Ok(list_page(
        conn,
        &HistoryList {
            connection_id: Some(connection_id.to_string()),
            limit,
            search: None,
            cursor_ran_at: None,
            cursor_id: None,
        },
    )?
    .items)
}

pub fn list_recent(conn: &Connection, limit: i64) -> AppResult<Vec<QueryHistoryItem>> {
    Ok(list_page(
        conn,
        &HistoryList {
            connection_id: None,
            limit,
            search: None,
            cursor_ran_at: None,
            cursor_id: None,
        },
    )?
    .items)
}

/// Drop the oldest rows for one connection. `None` caps rows with no connection.
pub fn retain_for_connection(
    conn: &Connection,
    connection_id: Option<&str>,
    keep: i64,
) -> AppResult<usize> {
    let id = parse_id_opt(connection_id)?;
    retain_connection(conn, id, keep)
}

fn retain_connection(conn: &Connection, connection_id: Option<i64>, keep: i64) -> AppResult<usize> {
    let keep = keep.max(0);
    let n = if let Some(id) = connection_id {
        conn.execute(
            "
            DELETE FROM query_history
            WHERE connection_id = ?1
              AND id NOT IN (
                SELECT id FROM query_history
                WHERE connection_id = ?1
                ORDER BY ran_at DESC, id DESC
                LIMIT ?2
              )
            ",
            params![id, keep],
        )?
    } else {
        conn.execute(
            "
            DELETE FROM query_history
            WHERE connection_id IS NULL
              AND id NOT IN (
                SELECT id FROM query_history
                WHERE connection_id IS NULL
                ORDER BY ran_at DESC, id DESC
                LIMIT ?1
              )
            ",
            params![keep],
        )?
    };
    Ok(n)
}

fn stored_database(value: Option<&str>) -> Option<String> {
    let name = value?.trim().to_string();
    if name.is_empty() || name.len() > DATABASE_MAX || name.contains("://") || name.contains('@') {
        return None;
    }
    Some(name)
}

fn clip_chars(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut clipped = text[..end].to_string();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::QueryHistorySource;
    use crate::storage::sqlite::migrate;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().expect("memory");
        conn.execute_batch("PRAGMA foreign_keys = ON;").expect("fk");
        migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO connections (name, engine, color_id, environment, read_only, plugin_config, created_at, updated_at)
             VALUES ('t', 'sqlite', 'slate', 'local', 0, '{}', 'now', 'now')",
            [],
        )
        .expect("connection");
        conn.execute(
            "INSERT INTO connections (name, engine, color_id, environment, read_only, plugin_config, created_at, updated_at)
             VALUES ('other', 'sqlite', 'slate', 'local', 0, '{}', 'now', 'now')",
            [],
        )
        .expect("other");
        conn
    }

    fn append_body(conn: &Connection, connection_id: Option<&str>, body: &str) -> QueryHistoryItem {
        append(
            conn,
            AppendHistoryInput {
                connection_id: connection_id.map(str::to_string),
                engine: Some("sqlite".into()),
                body: body.into(),
                language: Some("sql".into()),
                success: true,
                row_count: None,
                duration_ms: Some(1),
                error_message: None,
                database: Some("app".into()),
                affected_rows: Some(1),
                source: QueryHistorySource::Ui,
            },
        )
        .expect("append")
    }

    #[test]
    fn search_escapes_percent_and_underscore() {
        let conn = setup();
        append_body(&conn, Some("1"), "100%");
        append_body(&conn, Some("1"), "100x");
        append_body(&conn, Some("1"), "a_b");
        append_body(&conn, Some("1"), "axb");
        append_body(&conn, Some("1"), "plain");

        let percent = list_page(
            &conn,
            &HistoryList {
                connection_id: Some("1".into()),
                limit: 20,
                search: Some("100%".into()),
                cursor_ran_at: None,
                cursor_id: None,
            },
        )
        .expect("percent");
        assert_eq!(percent.items.len(), 1);
        assert_eq!(percent.items[0].body, "100%");

        let wild = list_page(
            &conn,
            &HistoryList {
                connection_id: Some("1".into()),
                limit: 20,
                search: Some("%".into()),
                cursor_ran_at: None,
                cursor_id: None,
            },
        )
        .expect("wild percent");
        assert_eq!(wild.items.len(), 1);
        assert_eq!(wild.items[0].body, "100%");

        let underscore = list_page(
            &conn,
            &HistoryList {
                connection_id: Some("1".into()),
                limit: 20,
                search: Some("_".into()),
                cursor_ran_at: None,
                cursor_id: None,
            },
        )
        .expect("underscore");
        assert_eq!(underscore.items.len(), 1);
        assert_eq!(underscore.items[0].body, "a_b");

        let literal = list_page(
            &conn,
            &HistoryList {
                connection_id: Some("1".into()),
                limit: 20,
                search: Some("a_b".into()),
                cursor_ran_at: None,
                cursor_id: None,
            },
        )
        .expect("literal");
        assert_eq!(literal.items.len(), 1);
        assert_eq!(literal.items[0].body, "a_b");
    }

    #[test]
    fn keyset_page_two_stays_put_when_a_newer_row_is_inserted() {
        let conn = setup();
        let oldest = append_body(&conn, Some("1"), "oldest");
        let middle = append_body(&conn, Some("1"), "middle");
        let newest = append_body(&conn, Some("1"), "newest");

        let page1 = list_page(
            &conn,
            &HistoryList {
                connection_id: Some("1".into()),
                limit: 2,
                search: None,
                cursor_ran_at: None,
                cursor_id: None,
            },
        )
        .expect("page 1");
        assert!(page1.has_more);
        assert_eq!(page1.items.len(), 2);
        assert_eq!(page1.items[0].id, newest.id);
        assert_eq!(page1.items[1].id, middle.id);

        let inserted = append_body(&conn, Some("1"), "inserted-after-page-1");
        let cursor = &page1.items[1];
        let page2 = list_page(
            &conn,
            &HistoryList {
                connection_id: Some("1".into()),
                limit: 2,
                search: None,
                cursor_ran_at: Some(cursor.ran_at.clone()),
                cursor_id: Some(cursor.id.clone()),
            },
        )
        .expect("page 2");
        assert_eq!(page2.items.len(), 1, "page 2 repeated or skipped a row");
        assert_eq!(page2.items[0].id, oldest.id);
        assert!(!page2.items.iter().any(|row| row.id == inserted.id));
        assert!(!page2.items.iter().any(|row| row.id == newest.id));
        assert!(!page2.items.iter().any(|row| row.id == middle.id));
    }

    #[test]
    fn retain_drops_the_oldest_rows_for_that_connection_only() {
        let conn = setup();
        let _other = append_body(&conn, Some("2"), "keep-other");
        append_body(&conn, Some("1"), "old");
        append_body(&conn, Some("1"), "mid");
        append_body(&conn, Some("1"), "new");
        let removed = retain_for_connection(&conn, Some("1"), 2).expect("retain");
        assert_eq!(removed, 1);
        let kept = list_for_connection(&conn, "1", 10).expect("list");
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|row| row.body != "old"));
        assert!(kept.iter().any(|row| row.body == "new"));
        let other = list_for_connection(&conn, "2", 10).expect("other");
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].body, "keep-other");
    }

    #[test]
    fn append_keeps_the_cap_and_drops_the_oldest() {
        let conn = setup();
        let _other = append_body(&conn, Some("2"), "other-connection");
        let extra = QUERY_HISTORY_CAP + 1;
        for i in 0..extra {
            append_body(&conn, Some("1"), &format!("row-{i}"));
        }
        let kept = list_for_connection(&conn, "1", 500).expect("list");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM query_history WHERE connection_id = 1",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, QUERY_HISTORY_CAP);
        assert_eq!(kept.len(), 500);
        assert!(kept.iter().all(|row| row.body != "row-0"));
        assert!(kept
            .iter()
            .any(|row| row.body == format!("row-{QUERY_HISTORY_CAP}")));
        let other: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM query_history WHERE connection_id = 2",
                [],
                |row| row.get(0),
            )
            .expect("other count");
        assert_eq!(other, 1);
    }

    #[test]
    fn retain_caps_rows_with_no_connection() {
        let conn = setup();
        append_body(&conn, None, "n1");
        append_body(&conn, None, "n2");
        append_body(&conn, None, "n3");
        let removed = retain_for_connection(&conn, None, 2).expect("retain");
        assert_eq!(removed, 1);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM query_history WHERE connection_id IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 2);
        let newest: String = conn
            .query_row(
                "SELECT body FROM query_history WHERE connection_id IS NULL ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("newest");
        assert_eq!(newest, "n3");
    }

    #[test]
    fn append_drops_uri_userinfo_and_a_database_uri() {
        let conn = setup();
        let item = append(
            &conn,
            AppendHistoryInput {
                connection_id: Some("1".into()),
                engine: Some("sqlite".into()),
                body: "SELECT 'mongodb://alice:secret-token@127.0.0.1/app'".into(),
                language: Some("sql".into()),
                success: false,
                row_count: None,
                duration_ms: Some(4),
                error_message: Some("failed mongodb://alice:secret-token@127.0.0.1/app".into()),
                database: Some("mongodb://alice:secret-token@127.0.0.1/app".into()),
                affected_rows: None,
                source: QueryHistorySource::Mcp,
            },
        )
        .expect("append");
        assert!(item.body.contains("SELECT"));
        assert!(item.body.contains("alice:***@"));
        assert!(!item.body.contains("secret-token"));
        assert!(item
            .error_message
            .as_deref()
            .is_some_and(|msg| !msg.contains("secret-token")));
        assert!(item.database.is_none());
        assert_eq!(item.source, QueryHistorySource::Mcp);
        let stored: String = conn
            .query_row(
                "SELECT body || ifnull(error_message,'') || ifnull(\"database\",'') FROM query_history",
                [],
                |row| row.get(0),
            )
            .expect("stored");
        assert!(!stored.contains("secret-token"));
    }
}
