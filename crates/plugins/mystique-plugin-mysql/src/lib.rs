//! MySQL / MariaDB engine plugin for Mystique.
//!
//! Pooling: [`mysql_async::Pool`].

mod connector;
mod session;

pub use connector::MysqlConnector;

use std::sync::Arc;

use mystique_connector_api::DatabaseConnector;

/// Construct the connector for registry bootstrap.
pub fn connector() -> Arc<dyn DatabaseConnector> {
    Arc::new(MysqlConnector)
}
