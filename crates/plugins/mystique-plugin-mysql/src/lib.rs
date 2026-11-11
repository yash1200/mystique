//! MySQL / MariaDB engine plugin for Mystique.
//!
//! Pooling: [`mysql_async::Pool`].

#![deny(clippy::indexing_slicing, clippy::unwrap_used)]

mod capabilities;
mod connector;
mod rows;
mod session;

pub use capabilities::default_capabilities;
pub use connector::MysqlConnector;

use std::sync::Arc;

use mystique_connector_api::DatabaseConnector;

/// Construct the connector for registry bootstrap.
pub fn connector() -> Arc<dyn DatabaseConnector> {
    Arc::new(MysqlConnector)
}
