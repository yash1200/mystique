//! PostgreSQL engine plugin for MystiqueDB.
//!
//! Pooling: [`deadpool_postgres`] over [`tokio_postgres`].

mod connector;
mod session;

pub use connector::PostgresConnector;

use std::sync::Arc;

use mystique_connector_api::DatabaseConnector;

/// Construct the connector for registry bootstrap.
pub fn connector() -> Arc<dyn DatabaseConnector> {
    Arc::new(PostgresConnector)
}
