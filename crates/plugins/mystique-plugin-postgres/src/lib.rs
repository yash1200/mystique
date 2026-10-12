//! PostgreSQL engine plugin for Mystique.
//!
//! Pooling: [`deadpool_postgres`] over [`tokio_postgres`].

#![deny(clippy::indexing_slicing, clippy::unwrap_used)]

mod connector;
mod session;

pub use connector::PostgresConnector;

use std::sync::Arc;

use mystique_connector_api::DatabaseConnector;

/// Construct the connector for registry bootstrap.
pub fn connector() -> Arc<dyn DatabaseConnector> {
    Arc::new(PostgresConnector)
}
