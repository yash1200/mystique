//! MongoDB engine plugin — official driver manages the connection pool.

mod connector;
mod session;
mod shell_filter;

pub use connector::MongoConnector;

use std::sync::Arc;

use mystique_connector_api::DatabaseConnector;

pub fn connector() -> Arc<dyn DatabaseConnector> {
    Arc::new(MongoConnector)
}
