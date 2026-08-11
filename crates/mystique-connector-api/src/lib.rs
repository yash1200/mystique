//! # mystique-connector-api
//!
//! Stable plugin contract for MystiqueDB engines.
//!
//! - **Plugins** implement [`DatabaseConnector`] + [`ConnectorSession`].
//! - **Core** depends only on this crate (never on engine drivers).
//! - Adding a new database = new plugin crate + one registry line.
//!
//! ## Connection pools
//!
//! Each plugin owns its pooling strategy inside the session returned by
//! [`DatabaseConnector::connect`]. Core only tracks sessions via
//! [`PoolStatus`] / session ids.

mod capabilities;
mod catalog;
mod config;
mod connector;
mod edits;
mod error;
mod form;
mod policy;
mod pool;
mod query;
mod sql_table_edits;
mod table_filters;
mod types;

pub use capabilities::*;
pub use catalog::*;
pub use config::*;
pub use connector::*;
pub use edits::*;
pub use error::*;
pub use form::*;
pub use policy::*;
pub use pool::*;
pub use query::*;
pub use sql_table_edits::{
    compile_delete, compile_insert, compile_update, qualify_table, quote_ident, SqlIdentStyle,
};
pub use table_filters::{mongo_filter_json, sql_where_clause};
pub use types::*;
