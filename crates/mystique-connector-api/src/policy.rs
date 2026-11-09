//! Per-engine query policy SPI.
//!
//! Core orchestrates validate → execute. Each engine owns its own implementation.
//! The MongoDB policy lives in its plugin, next to the shell parser the
//! executor uses (`mystique_plugin_mongodb::query_policy`).

mod sql;

use crate::error::ConnectorResult;
use crate::query::QueryMode;
use crate::WriteRisk;

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

    /// INSERT / UPDATE only: the MCP / agent `safe_write` tier.
    /// Default: nothing is, so an engine has to opt in.
    fn is_safe_dml(&self, _text: &str) -> bool {
        false
    }

    /// Byte ranges of the statements in `text`, in order, for engines whose
    /// batches core runs one statement at a time (MongoDB). Default: the whole
    /// text is one statement. SQL engines are planned by `mystique-sql`
    /// instead and do not use this.
    fn split(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        vec![std::ops::Range {
            start: 0,
            end: text.len(),
        }]
    }

    /// Short name for the write-guard prompt. Default: the text, compacted.
    fn op_preview(&self, text: &str) -> String {
        let compact: String = text
            .chars()
            .filter(|c| !c.is_whitespace())
            .take(80)
            .collect();
        if compact.is_empty() {
            "write".into()
        } else {
            compact
        }
    }

    /// Databases the writes in `text` land in, for per-database production.
    /// `current` is the database the statement runs in. Default: a read
    /// names nothing, anything else is unresolved (fail closed).
    fn write_targets(&self, text: &str, current: Option<&str>) -> crate::WriteTargets {
        let _ = current;
        if self.classify(text) == WriteRisk::Read {
            crate::WriteTargets::default()
        } else {
            crate::WriteTargets::unresolved()
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

    fn is_safe_dml(&self, text: &str) -> bool {
        sql_is_safe_dml_for(text, self.dialect())
    }

    fn write_targets(&self, text: &str, current: Option<&str>) -> crate::WriteTargets {
        mystique_sql::write_targets(text, self.dialect(), current)
    }
}
