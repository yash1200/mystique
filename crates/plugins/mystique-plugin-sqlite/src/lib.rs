//! SQLite engine plugin — file path based, r2d2 connection pool.

mod connector;
mod session;

pub use connector::SqliteConnector;

use std::sync::Arc;

use mystique_connector_api::DatabaseConnector;

pub fn connector() -> Arc<dyn DatabaseConnector> {
    Arc::new(SqliteConnector)
}
