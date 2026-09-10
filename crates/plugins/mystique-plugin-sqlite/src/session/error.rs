//! User-facing SQLite query errors (skip rusqlite's wrapper noise).

use mystique_connector_api::{prettify_sql_error, ConnectorError};

pub(crate) fn format_query_error(err: &rusqlite::Error) -> String {
    match err {
        rusqlite::Error::SqliteFailure(_, Some(msg)) => {
            let msg = msg.trim();
            if msg.is_empty() {
                prettify_sql_error(&err.to_string())
            } else {
                prettify_sql_error(msg)
            }
        }
        other => prettify_sql_error(&other.to_string()),
    }
}

pub(crate) fn query_failed(err: rusqlite::Error) -> ConnectorError {
    ConnectorError::Query(format_query_error(&err))
}

#[cfg(test)]
mod tests {
    use super::format_query_error;

    #[test]
    fn syntax_error_uses_sqlite_message() {
        let conn = rusqlite::Connection::open_in_memory().expect("memory db");
        let err = conn
            .execute("SELEC 1", [])
            .expect_err("invalid SQL should fail");
        let msg = format_query_error(&err);
        assert!(
            msg.to_ascii_lowercase().contains("syntax")
                || msg.to_ascii_lowercase().contains("selec"),
            "unexpected sqlite syntax error: {msg}"
        );
        assert!(!msg.to_ascii_lowercase().contains("query failed"));
    }

    #[test]
    fn missing_table_names_the_table() {
        let conn = rusqlite::Connection::open_in_memory().expect("memory db");
        let err = conn
            .execute("SELECT * FROM missing_table", [])
            .expect_err("missing table should fail");
        let msg = format_query_error(&err);
        assert!(
            msg.contains("missing_table"),
            "expected table name in error: {msg}"
        );
    }

    #[test]
    fn not_null_names_the_column() {
        let conn = rusqlite::Connection::open_in_memory().expect("memory db");
        conn.execute("CREATE TABLE t (content TEXT NOT NULL)", [])
            .expect("create");
        let err = conn
            .execute("INSERT INTO t (content) VALUES (NULL)", [])
            .expect_err("not null should fail");
        assert_eq!(format_query_error(&err), "\"content\" cannot be empty.");
    }
}
