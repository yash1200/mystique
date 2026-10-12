//! MongoDB engine plugin — official driver manages the connection pool.

#![deny(clippy::indexing_slicing, clippy::unwrap_used)]

mod connector;
mod preview_filter;
mod session;
mod shell_filter;
mod shell_query;

pub use connector::MongoConnector;

use std::sync::Arc;

use mystique_connector_api::DatabaseConnector;

pub fn connector() -> Arc<dyn DatabaseConnector> {
    Arc::new(MongoConnector)
}
