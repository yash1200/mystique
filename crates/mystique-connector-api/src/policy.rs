//! Per-engine query policy SPI.
//!
//! Core orchestrates validate → execute. Each engine owns its own implementation.

mod sql;

use crate::error::{ConnectorError, ConnectorResult};
use crate::query::QueryMode;
use crate::WriteRisk;

pub use sql::{
    classify_sql_statement, classify_sql_statement_for, split_sql_statements,
    split_sql_statements_for, sql_destructive_object_name, sql_destructive_object_name_for,
    sql_explain_executes, sql_is_safe_dml, sql_is_safe_dml_for, validate_sql_statement,
    validate_sql_statement_for, SqlDialect,
};

/// Engine-specific statement classification + allow/deny.
pub trait EngineQueryPolicy: Send + Sync {
    fn engine_id(&self) -> &'static str;

    /// Fail closed: unknown / unparsed → error.
    fn validate(&self, text: &str, mode: QueryMode) -> ConnectorResult<()>;

    /// AST risk for SQL engines. Default: Read if `validate(..., Read)` succeeds.
    fn classify(&self, text: &str) -> WriteRisk {
        if self.validate(text, QueryMode::Read).is_ok() {
            WriteRisk::Read
        } else {
            WriteRisk::Write
        }
    }
}

pub struct SqlQueryPolicy {
    id: &'static str,
}

impl SqlQueryPolicy {
    pub const fn new(id: &'static str) -> Self {
        Self { id }
    }

    fn dialect(&self) -> SqlDialect {
        SqlDialect::from_engine(self.id)
    }
}

impl EngineQueryPolicy for SqlQueryPolicy {
    fn engine_id(&self) -> &'static str {
        self.id
    }

    fn validate(&self, text: &str, mode: QueryMode) -> ConnectorResult<()> {
        validate_sql_statement_for(text, mode, self.dialect())
    }

    fn classify(&self, text: &str) -> WriteRisk {
        classify_sql_statement_for(text, self.dialect())
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

    fn classify(&self, text: &str) -> WriteRisk {
        classify_mongo_query(text)
    }
}

/// INSERT / UPDATE / REPLACE only (P3.2 `safe_write`). Deletes and `$out`/`$merge` are not.
pub fn mongo_is_safe_dml(text: &str) -> bool {
    if classify_mongo_query(text) != WriteRisk::Write {
        return false;
    }
    let lower = text.trim().to_ascii_lowercase();
    if lower.contains("delete") || lower.contains(".remove(") || lower.contains("bulkwrite") {
        return false;
    }
    lower.contains("insert") || lower.contains("update") || lower.contains("replace")
}

fn classify_mongo_query(text: &str) -> WriteRisk {
    let lower = text.trim().to_ascii_lowercase();
    if lower.contains("dropdatabase")
        || lower.contains("dropcollection")
        || lower.contains(".drop(")
        || lower.contains("$out")
        || lower.contains("$merge")
    {
        return WriteRisk::Destructive;
    }
    if lower.contains("insertone")
        || lower.contains("insertmany")
        || lower.contains("updateone")
        || lower.contains("updatemany")
        || lower.contains("replaceone")
        || lower.contains("bulkwrite")
        || lower.contains("deleteone")
        || lower.contains("deletemany")
        || lower.contains("delete ")
        || lower.contains(".remove(")
    {
        return WriteRisk::Write;
    }
    WriteRisk::Read
}

#[cfg(test)]
mod mongo_safe_dml_tests {
    use super::*;

    #[test]
    fn insert_is_safe_dml() {
        assert!(mongo_is_safe_dml("db.t.insertOne({a:1})"));
        assert!(mongo_is_safe_dml("db.t.updateOne({a:1}, {$set:{b:2}})"));
    }

    #[test]
    fn delete_and_drop_are_not_safe_dml() {
        assert!(!mongo_is_safe_dml("db.t.deleteOne({a:1})"));
        assert!(!mongo_is_safe_dml("db.t.drop()"));
        assert!(!mongo_is_safe_dml("db.t.find({})"));
    }
}
