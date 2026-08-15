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

pub use application::{
    tool_specs, AgentColumnInfo, AgentConnectionInfo, AgentObjectInfo, AgentQueryDownload,
    AgentToolSpec, AgentTools, AppServices,
};
pub use connectors::{ConnectionManager, ConnectorRegistry, OpenSessionRequest};
pub use domain::*;
pub use error::{AppError, AppResult};
pub use storage::AppDatabase;

// Re-export API types for IPC adapters
pub use mystique_connector_api::{
    ApplyTableEditsRequest, ApplyTableEditsResult, CatalogColumnRow, CatalogDatabase,
    CatalogObject, CatalogRelationship, ColumnInfo, ColumnReference, ConnectionConfig,
    DescribeTableRequest, EngineCapabilities, EngineDescriptor, ExecuteQueryRequest,
    ExportFormat, ExportObjectRequest, ExportObjectResult, FetchTablePreviewRequest,
    ImportObjectRequest, ImportObjectResult, PreviewImportRequest, PreviewImportResult,
    ListCatalogColumnsRequest, ListCatalogRequest, ListRelationshipsRequest, ObjectStructure,
    ObjectStructureRequest, PoolConfig, PoolStatus, QueryColumn, QueryMode, QueryResult,
    SessionInfo, StructureSection, TableAdminOp, TableAdminRequest, TableAdminResult,
    TableEditColumn, TablePreviewResult, TableRowInsert, TableRowUpdate, TestResult,
};
