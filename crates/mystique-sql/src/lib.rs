//! Shared SQL: statement split, AST write-risk, identifier quoting, and the
//! statement builders (grid DML, filters, structure DDL).
//!
//! No app domain and no engine drivers. Plugins and `mystique-connector-api`
//! call this crate so the query tab, agent, and MCP share one lock.
//! This crate must not depend on `mystique-connector-api`; the SPI re-exports
//! from here and maps [`BuildError`] onto its own error type.

pub mod ddl;
mod dialect;
pub mod dml;
mod error;
pub mod filters;
mod ident;
mod literal;
mod params;
mod plan;
mod projection;
mod read_proof;
mod risk;
mod split;
mod switch;
mod tautology;

pub use dialect::{SqlDialect, SqlPlaceholder};
pub use error::{BuildError, BuildResult};
pub use ident::{qualify_table, quote_ident, quote_path};
pub use literal::{sql_literal, sql_string_literal};
pub use params::{mask_placeholders, ParamSlot, ScriptParams};
pub use plan::{first_syntax_fault, plan_batch, PlannedStatement, SyntaxFault};
pub use projection::{single_table_select, ProjectedItem, SingleTableSelect};
pub use read_proof::{read_proof, read_proof_statement, DenyKind, ReadProofError, ReadProofReason};
pub use risk::{
    classify, destructive_object_name, explain_executes, explain_subject, is_read_safe_set,
    is_safe_dml, StatementRisk,
};
pub use split::{split_statement_spans, split_statements, statement_at, StatementSpan};
pub use switch::switches_database;
pub use tautology::where_warnings;
