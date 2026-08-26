use crate::error::{ConnectorError, ConnectorResult};
use crate::query::QueryMode;

// ── Shared SQL helpers (Postgres / MySQL / SQLite) ──────────────────────────

fn strip_sql_comments(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            out.push(' ');
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'-' && bytes[i + 1] == b'-' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            out.push(' ');
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn first_sql_keyword(sql: &str) -> Option<String> {
    let cleaned = strip_sql_comments(sql);
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return None;
    }
    let word: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_alphabetic() || *c == '_')
        .collect::<String>()
        .to_ascii_lowercase();
    if word.is_empty() {
        None
    } else {
        Some(word)
    }
}

/// Split a SQL batch into individual statements (`;`-separated, comments stripped).
/// Multi-statement batches are allowed; callers should run each and keep the last result.
pub fn split_sql_statements(sql: &str) -> Vec<String> {
    let cleaned = strip_sql_comments(sql);
    cleaned
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

const SQL_READ: &[&str] = &[
    "select", "with", "show", "describe", "desc", "explain", "values", "table", "pragma",
];
/// Session / connection utilities (search_path, timezone, USE db, etc.) — allowed in read mode.
const SQL_SESSION: &[&str] = &[
    "set",
    "reset",
    "use",
    "begin",
    "start",
    "commit",
    "rollback",
    "savepoint",
    "release",
    "discard",
    "deallocate",
    "prepare",
    "execute",
    "listen",
    "unlisten",
    "notify",
];
/// Inserts, updates, deletes, and DDL — allowed unless the connection is read-only.
const SQL_WRITE: &[&str] = &[
    "insert", "update", "replace", "merge", "delete", "truncate", "drop", "alter", "create",
    "grant", "revoke", "comment", "vacuum", "reindex", "attach", "detach",
];

fn validate_one_sql_statement(text: &str, mode: QueryMode) -> ConnectorResult<()> {
    let Some(kw) = first_sql_keyword(text) else {
        return Err(ConnectorError::Policy(
            "Could not classify this SQL. Blocked (fail closed).".into(),
        ));
    };
    if SQL_READ.contains(&kw.as_str()) {
        return Ok(());
    }
    // e.g. SET search_path TO content; — session config, not a data write.
    if SQL_SESSION.contains(&kw.as_str()) {
        return Ok(());
    }
    if SQL_WRITE.contains(&kw.as_str()) {
        if mode == QueryMode::Read {
            return Err(ConnectorError::Policy(format!(
                "Write statement ({}) blocked. This connection is read-only.",
                kw.to_ascii_uppercase()
            )));
        }
        return Ok(());
    }
    Err(ConnectorError::Policy(format!(
        "Statement type “{}” is not allowed in this mode.",
        kw.to_ascii_uppercase()
    )))
}

/// Shared SQL policy used by relational engines (dialect-agnostic keyword gate).
/// Multi-statement batches are allowed; every statement is validated.
pub fn validate_sql_statement(text: &str, mode: QueryMode) -> ConnectorResult<()> {
    let stmts = split_sql_statements(text);
    if stmts.is_empty() {
        return Err(ConnectorError::Policy("Query is empty.".into()));
    }
    for stmt in &stmts {
        validate_one_sql_statement(stmt, mode)?;
    }
    Ok(())
}
