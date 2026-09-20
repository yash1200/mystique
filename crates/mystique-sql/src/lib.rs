//! Shared SQL analysis: statement split, AST write-risk, identifier quoting.
//!
//! No app domain and no engine drivers. Plugins and `mystique-connector-api`
//! call this crate so the query tab, agent, and MCP share one lock.

mod dialect;
mod ident;
mod risk;
mod split;

pub use dialect::SqlDialect;
pub use ident::{quote_ident, quote_path};
pub use risk::{classify, destructive_object_name, is_safe_dml, StatementRisk};
pub use split::split_statements;
