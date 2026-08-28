//! User-facing SQLite query errors (skip rusqlite's wrapper noise).

use mystique_connector_api::ConnectorError;

pub(crate) fn format_query_error(err: &rusqlite::Error) -> String {
    match err {
        rusqlite::Error::SqliteFailure(_, Some(msg)) => {
            let msg = msg.trim();
            if msg.is_empty() {
                err.to_string()
            } else {
                msg.to_string()
            }
        }
        other => other.to_string(),
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
}
