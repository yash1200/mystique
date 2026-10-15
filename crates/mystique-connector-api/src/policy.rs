//! Per-engine query policy SPI.
//!
//! Core orchestrates validate → execute. Each engine owns its own implementation.

mod mongo;
mod sql;

use crate::error::ConnectorResult;
use crate::query::QueryMode;
use crate::WriteRisk;

pub use mongo::{mongo_is_safe_dml, mongo_is_write_command, MongoQueryPolicy};
pub use sql::{
    classify_sql_statement, classify_sql_statement_for, split_sql_statements,
    split_sql_statements_for, sql_analyze_guard_risk, sql_destructive_object_name,
    sql_destructive_object_name_for, sql_explain_executes, sql_explain_subject, sql_is_proven_read,
    sql_is_safe_dml, sql_is_safe_dml_for, validate_sql_statement, validate_sql_statement_for,
    SqlDialect,
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
