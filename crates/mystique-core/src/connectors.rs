//! Engine registry and live session management.
//!
//! Core never imports engine drivers — only [`mystique_connector_api`]
//! and plugin entry points registered at bootstrap.

mod manager;
mod registry;
mod ssh;

pub use manager::{ConnectionManager, OpenSessionRequest};
pub use registry::ConnectorRegistry;
