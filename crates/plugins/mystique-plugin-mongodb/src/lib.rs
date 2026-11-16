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

/// Internals for `tests/` (feature `test-support`, which only this crate's
/// dev-dependency on itself turns on): the session and the shell parser.
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support {
    pub use crate::session::MongoSession;
    pub use crate::shell::parse::{
        parse, parse_executable, DeleteModel, Executable, FindAndModify, Listing, Modify,
        MongoCall, MongoOp, ReadOptions, ShellWrite, Statement, Target, UpdateModel, WriteMethod,
        WriteModel, WriteOptions,
    };
    pub use crate::shell::value::parse_document;
}

use std::sync::Arc;

use mystique_connector_api::{DatabaseConnector, EngineQueryPolicy};

pub fn connector() -> Arc<dyn DatabaseConnector> {
    Arc::new(MongoConnector)
}

/// Write policy for Mongo text. It reads the same parse the executor runs.
pub fn query_policy() -> Arc<dyn EngineQueryPolicy> {
    Arc::new(MongoQueryPolicy)
}
