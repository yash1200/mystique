//! Per-engine query policy SPI.
//!
//! Core orchestrates validate → execute. Each engine owns its own implementation.

mod sql;

use crate::error::{ConnectorError, ConnectorResult};
use crate::query::QueryMode;

pub use sql::{split_sql_statements, validate_sql_statement};

/// Engine-specific statement classification + allow/deny.
pub trait EngineQueryPolicy: Send + Sync {
    fn engine_id(&self) -> &'static str;

    /// Fail closed: unknown / unparsed → error.
    fn validate(&self, text: &str, mode: QueryMode) -> ConnectorResult<()>;
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

        let is_write = lower.contains("insertone")
            || lower.contains("insertmany")
            || lower.contains("updateone")
            || lower.contains("updatemany")
            || lower.contains("replaceone")
            || lower.contains("bulkwrite")
            || lower.contains("deleteone")
            || lower.contains("deletemany")
            || lower.contains("delete ")
            || lower.contains(".drop(")
            || lower.contains("dropdatabase")
            || lower.contains("dropcollection")
            || lower.contains("$out")
            || lower.contains("$merge")
            || lower.contains(".remove(");

        if is_write {
            if mode == QueryMode::Read {
                return Err(ConnectorError::Policy(
                    "Write operation blocked. This connection is read-only.".into(),
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
