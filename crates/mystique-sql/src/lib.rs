//! Shared SQL analysis: statement split, AST write-risk, identifier quoting.
//!
//! No app domain and no engine drivers. Plugins and `mystique-connector-api`
//! call this crate so the query tab, agent, and MCP share one lock.

mod dialect;
mod ident;
mod params;
mod plan;
mod risk;
mod split;

pub use dialect::SqlDialect;
pub use ident::{quote_ident, quote_path};
pub use params::{mask_placeholders, ParamSlot, ScriptParams};
pub use plan::{first_syntax_fault, plan_batch, PlannedStatement, SyntaxFault};
pub use risk::{
    classify, destructive_object_name, explain_executes, explain_subject, is_safe_dml,
    is_single_row_dml, StatementRisk,
};
pub use split::{split_statement_spans, split_statements, statement_at, StatementSpan};
