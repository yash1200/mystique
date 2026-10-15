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
pub mod secrets;
pub mod storage;

pub use application::{
    parse_allowlist, tool_specs, tool_specs_for, AgentColumnInfo, AgentConnectionInfo,
    AgentObjectInfo, AgentQueryDownload, AgentToolSpec, AgentTools, AgentWritePolicy, AppServices,
    ToolCaller, AGENT_DEFAULT_CELL_CHARS, AGENT_MAX_CELL_CHARS, AGENT_MAX_ROWS,
    SETTING_MCP_ALLOWLIST, SETTING_MCP_POLICY,
};
pub use connectors::{ConnectionManager, ConnectorRegistry, OpenSessionRequest};
pub use domain::*;
pub use error::{AppError, AppResult};
pub use secrets::SecretStore;
pub use storage::AppDatabase;

// Re-export API types for IPC adapters
pub use mystique_connector_api::{
    ApplyTableEditsRequest, ApplyTableEditsResult, BatchEvent, CatalogColumnRow, CatalogDatabase,
    CatalogObject, CatalogRelationship, ColumnInfo, ColumnReference, ConnectionConfig,
    ConnectorError, ConnectorResult, DescribeTableRequest, EngineCapabilities, EngineDescriptor,
    ExecuteQueryRequest, ExportFormat, ExportObjectRequest, ExportObjectResult,
    FetchServerMetricsRequest, FetchTablePreviewRequest, FetchTablePreviewStatsRequest,
    FormattedConnectionUrl, ImportObjectRequest, ImportObjectResult, ListCatalogColumnsRequest,
    ListCatalogRequest, ListRelationshipsRequest, ObjectStructure, ObjectStructureRequest,
    ParsedConnectionUrl, PinStatus, PoolConfig, PoolStatus, PreviewImportRequest,
    PreviewImportResult, ProgressPump, QueryBatchResult, QueryColumn, QueryIpcError, QueryMode,
    QueryResult, ServerMetricsSnapshot, SessionInfo, SshHostKeyPrompt, StructureSection,
    TableAdminOp, TableAdminRequest, TableAdminResult, TableEditColumn, TablePreviewColumnRef,
    TablePreviewResult, TablePreviewStats, TableRowInsert, TableRowUpdate, TerminateSessionRequest,
    TerminateSessionResult, TestResult,
};
