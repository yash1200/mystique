//! Per-engine query policy SPI.
//!
//! Core orchestrates validate → execute. Each engine owns its own implementation.

use crate::error::{ConnectorError, ConnectorResult};
use crate::query::QueryMode;

/// Engine-specific statement classification + allow/deny.
pub trait EngineQueryPolicy: Send + Sync {
    fn engine_id(&self) -> &'static str;

    /// Fail closed: unknown / unparsed → error.
    fn validate(&self, text: &str, mode: QueryMode) -> ConnectorResult<()>;
}

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

fn multi_statement_sql(sql: &str) -> bool {
    let cleaned = strip_sql_comments(sql);
    cleaned
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .count()
        > 1
}

const SQL_READ: &[&str] = &[
    "select", "with", "show", "describe", "desc", "explain", "values", "table", "pragma",
];
const SQL_WRITE: &[&str] = &["insert", "update", "replace", "merge"];
const SQL_DELETE: &[&str] = &[
    "delete", "truncate", "drop", "alter", "create", "grant", "revoke", "comment", "vacuum",
    "reindex", "attach", "detach",
];

/// Shared SQL policy used by relational engines (dialect-agnostic keyword gate).
pub fn validate_sql_statement(text: &str, mode: QueryMode) -> ConnectorResult<()> {
    let cleaned = strip_sql_comments(text);
    if cleaned.trim().is_empty() {
        return Err(ConnectorError::Policy("Query is empty.".into()));
    }
    if multi_statement_sql(text) {
        return Err(ConnectorError::Policy(
            "Multi-statement SQL is not allowed. Run one statement at a time.".into(),
        ));
    }
    let Some(kw) = first_sql_keyword(text) else {
        return Err(ConnectorError::Policy(
            "Could not classify this SQL. Blocked (fail closed).".into(),
        ));
    };
    if SQL_DELETE.contains(&kw.as_str()) {
        return Err(ConnectorError::Policy(
            "DELETE and other destructive statements are disabled for now.".into(),
        ));
    }
    if SQL_READ.contains(&kw.as_str()) {
        return Ok(());
    }
    if SQL_WRITE.contains(&kw.as_str()) {
        if mode == QueryMode::Read {
            return Err(ConnectorError::Policy(format!(
                "Write statement ({}) blocked. Enable “Allow writes” to run inserts/updates.",
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

pub struct SqlQueryPolicy {
    id: &'static str,
}

impl SqlQueryPolicy {
    pub const fn new(id: &'static str) -> Self {
        Self { id }
    }
}

impl EngineQueryPolicy for SqlQueryPolicy {
    fn engine_id(&self) -> &'static str {
        self.id
    }

    fn validate(&self, text: &str, mode: QueryMode) -> ConnectorResult<()> {
        validate_sql_statement(text, mode)
    }
}

/// MongoDB / document policy.
pub struct MongoQueryPolicy;

impl EngineQueryPolicy for MongoQueryPolicy {
    fn engine_id(&self) -> &'static str {
        "mongodb"
    }

    fn validate(&self, text: &str, mode: QueryMode) -> ConnectorResult<()> {
        let t = text.trim();
        if t.is_empty() {
            return Err(ConnectorError::Policy("Query is empty.".into()));
        }
        let lower = t.to_ascii_lowercase();

        if lower.contains("deleteone")
            || lower.contains("deletemany")
            || lower.contains("delete ")
            || lower.contains(".drop(")
            || lower.contains("dropdatabase")
            || lower.contains("dropcollection")
            || lower.contains("$out")
            || lower.contains("$merge")
            || lower.contains(".remove(")
        {
            return Err(ConnectorError::Policy(
                "Delete/drop and write stages ($out / $merge) are disabled for now.".into(),
            ));
        }

        let is_write = lower.contains("insertone")
            || lower.contains("insertmany")
            || lower.contains("updateone")
            || lower.contains("updatemany")
            || lower.contains("replaceone")
            || lower.contains("bulkwrite");

        if is_write {
            if mode == QueryMode::Read {
                return Err(ConnectorError::Policy(
                    "Write operation blocked. Enable “Allow writes” to run inserts/updates.".into(),
                ));
            }
            return Ok(());
        }

        // Catalog meta (explorer refresh)
        if matches!(
            lower.as_str(),
            "listdatabases"
                | "list databases"
                | "show databases"
                | "show dbs"
                | "listcollections"
                | "list collections"
                | "show collections"
        ) {
            return Ok(());
        }

        // Reads: find / aggregate / bare JSON filter
        if lower.contains("find")
            || lower.contains("aggregate")
            || lower.contains("count")
            || t.starts_with('{')
            || t.starts_with('[')
        {
            return Ok(());
        }

        Err(ConnectorError::Policy(
            "Could not classify this Mongo query. Use a JSON filter, find(...), listDatabases, or listCollections."
                .into(),
        ))
    }
}
