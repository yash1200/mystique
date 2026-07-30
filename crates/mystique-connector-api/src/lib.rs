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
mod error;
mod form;
mod policy;
mod pool;
mod query;
mod types;

pub use capabilities::*;
pub use catalog::*;
pub use config::*;
pub use connector::*;
pub use error::*;
pub use form::*;
pub use policy::*;
pub use pool::*;
pub use query::*;
pub use types::*;
