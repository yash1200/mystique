//! MongoDB engine plugin — official driver manages the connection pool.

#![deny(clippy::indexing_slicing, clippy::unwrap_used)]

mod capabilities;
mod connector;
mod preview_filter;
mod session;
mod shell;

pub use capabilities::default_capabilities;
pub use connector::MongoConnector;
pub use shell::MongoQueryPolicy;

use std::sync::Arc;

use mystique_connector_api::{DatabaseConnector, EngineQueryPolicy};

pub fn connector() -> Arc<dyn DatabaseConnector> {
    Arc::new(MongoConnector)
}

/// Write policy for Mongo text. It reads the same parse the executor runs.
pub fn query_policy() -> Arc<dyn EngineQueryPolicy> {
    Arc::new(MongoQueryPolicy)
}
