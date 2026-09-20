//! # mystique-connector-api
//!
//! Stable plugin contract for Mystique engines.
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
mod metrics;
mod object_structure;
mod policy;
mod pool;
mod products;
mod query;
mod table;
mod tls;
mod types;
mod write_guard;

pub use capabilities::*;
pub use catalog::*;
pub use config::*;
pub use connector::*;
pub use error::*;
pub use form::*;
pub use metrics::*;
pub use object_structure::*;
pub use policy::*;
pub use pool::*;
pub use products::{product, runtime_id, EngineProduct, ENGINE_PRODUCTS};
pub use query::*;
pub use table::*;
pub use tls::{SslMode, TlsSettings};
pub use types::*;
pub use write_guard::{
    environment_needs_type_confirm, environment_needs_unlock, WriteGuardPrompt, WriteRisk,
    WRITE_GUARD_CODE,
};
