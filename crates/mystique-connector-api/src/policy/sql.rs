use crate::error::{ConnectorError, ConnectorResult};
use crate::query::QueryMode;
use crate::WriteRisk;

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
const SQL_DESTRUCTIVE: &[&str] = &["drop", "truncate"];

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

/// Worst risk across a SQL batch. Fail-closed unknown statements count as Write.
pub fn classify_sql_statement(text: &str) -> WriteRisk {
    let stmts = split_sql_statements(text);
    if stmts.is_empty() {
        return WriteRisk::Read;
    }
    let mut worst = WriteRisk::Read;
    for stmt in &stmts {
        let risk = match first_sql_keyword(stmt).as_deref() {
            Some(kw) if SQL_DESTRUCTIVE.contains(&kw) => WriteRisk::Destructive,
            Some(kw) if SQL_WRITE.contains(&kw) => WriteRisk::Write,
            Some(kw) if SQL_READ.contains(&kw) || SQL_SESSION.contains(&kw) => WriteRisk::Read,
            _ => WriteRisk::Write,
        };
        if risk > worst {
            worst = risk;
        }
    }
    worst
}

/// Last identifier after DROP/TRUNCATE for type-to-confirm.
pub fn sql_destructive_object_name(text: &str) -> Option<String> {
    let stmts = split_sql_statements(text);
    for stmt in stmts.iter().rev() {
        let Some(kw) = first_sql_keyword(stmt) else {
            continue;
        };
        if !SQL_DESTRUCTIVE.contains(&kw.as_str()) {
            continue;
        }
        if let Some(name) = last_sql_ident(stmt) {
            return Some(name);
        }
        return Some(kw.to_ascii_uppercase());
    }
    None
}

fn last_sql_ident(sql: &str) -> Option<String> {
    const SKIP: &[&str] = &[
        "drop",
        "truncate",
        "table",
        "index",
        "schema",
        "database",
        "view",
        "materialized",
        "sequence",
        "if",
        "exists",
        "only",
        "cascade",
        "restrict",
        "concurrently",
        "temporary",
        "temp",
    ];
    let cleaned = strip_sql_comments(sql);
    let mut last: Option<String> = None;
    for raw in cleaned
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '"' | '`' | '.'))
    {
        let word = raw.trim_matches(|c: char| matches!(c, '"' | '`' | '[' | ']' | '.'));
        if word.is_empty() {
            continue;
        }
        let lower = word.to_ascii_lowercase();
        if SKIP.contains(&lower.as_str()) {
            continue;
        }
        last = Some(word.to_string());
    }
    last
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_select_is_read() {
        assert_eq!(classify_sql_statement("SELECT 1"), WriteRisk::Read);
    }

    #[test]
    fn classify_delete_is_write() {
        assert_eq!(
            classify_sql_statement("DELETE FROM users"),
            WriteRisk::Write
        );
    }

    #[test]
    fn classify_drop_is_destructive() {
        assert_eq!(
            classify_sql_statement("DROP TABLE public.users"),
            WriteRisk::Destructive
        );
        assert_eq!(
            sql_destructive_object_name("DROP TABLE public.users").as_deref(),
            Some("users")
        );
    }

    #[test]
    fn classify_batch_takes_worst() {
        assert_eq!(
            classify_sql_statement("SELECT 1; DELETE FROM t;"),
            WriteRisk::Write
        );
        assert_eq!(
            classify_sql_statement("INSERT INTO t VALUES (1); DROP TABLE t;"),
            WriteRisk::Destructive
        );
    }
}
