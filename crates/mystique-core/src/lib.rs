//! # mystique-core
//!
//! Domain logic, local persistence, and engine connection management.
//!
//! ```text
//! domain/          Pure types (no I/O)
//! storage/         Local app SQLite (profiles, history, …)
//! application/     Use-case services
//! connectors/      Registry + ConnectionManager (engine pools)
//! ```
//!
//! Engine drivers live in plugin crates. Core only depends on
//! `mystique-connector-api` + plugin entry points.

pub mod application;
pub mod connectors;
pub mod domain;
pub mod error;
pub mod storage;

pub use application::AppServices;
pub use connectors::{ConnectionManager, ConnectorRegistry, OpenSessionRequest};
pub use domain::*;
pub use error::{AppError, AppResult};
pub use storage::AppDatabase;

// Re-export API types for IPC adapters
pub use mystique_connector_api::{
    ApplyTableEditsRequest, ApplyTableEditsResult, CatalogColumnRow, CatalogDatabase,
    CatalogObject, CatalogRelationship, ColumnInfo, ConnectionConfig, DescribeTableRequest,
    EngineCapabilities, EngineDescriptor, ExecuteQueryRequest, FetchTablePreviewRequest,
    ListCatalogColumnsRequest, ListCatalogRequest, ListRelationshipsRequest, PoolConfig,
    PoolStatus, QueryColumn, QueryMode, QueryResult, SessionInfo, TableAdminOp, TableAdminRequest,
    TableAdminResult, TableEditColumn, TablePreviewResult, TableRowInsert, TableRowUpdate,
    TestResult,
};
