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
pub mod diagnostics;
pub mod domain;
pub mod error;
pub mod redact;
pub mod secrets;
pub mod storage;

pub use application::{
    levels, tool_specs, tool_specs_for, AccessLevel, AgentColumnInfo, AgentConnectionInfo,
    AgentExplain, AgentObjectInfo, AgentQueryDownload, AgentSample, AgentSampleColumn,
    AgentToolSpec, AgentTools, AppServices, PurgeReport, ToolAccessService, ToolAnnotations,
    ToolApprovals, ToolCaller, VaultService, AGENT_DEFAULT_CELL_CHARS, AGENT_MAX_CELL_CHARS,
    AGENT_MAX_ROWS, AGENT_PLAN_MAX_NODES, AGENT_RAW_DEFAULT_CHARS, AGENT_SAMPLE_DEFAULT_ROWS,
    AGENT_SAMPLE_MAX_ROWS, APPROVAL_TIMEOUT, SETTING_AGENT_LEVEL, SETTING_MCP_LEVEL,
};
pub use connectors::{
    ConnectionManager, ConnectorRegistry, OpenSessionRequest, PreviewWriteRowsRequest,
    TimedUnlockStatus, WriteRowPreview, ROW_PREVIEW_LIMIT, TIMED_UNLOCK_CHOICES_SECS,
};
pub use domain::*;
pub use error::{AppError, AppResult};
pub use secrets::{KeySource, Prompt, SecretKind, SecretStore, VaultError, VaultKey, VaultStatus};
pub use storage::AppDatabase;

// Re-export API types for IPC adapters
pub use mystique_connector_api::{
    clip_batch_result, clip_query_result, clip_table_preview, ApplyDdlRequest, ApplyDdlResult,
    ApplySchemaDiffRequest, ApplyTableEditsRequest, ApplyTableEditsResult, BatchEvent,
    CatalogColumnRow, CatalogDatabase, CatalogNode, CatalogObject, CatalogPage, CatalogPath,
    CatalogRelationship, ColumnInfo, ColumnReference, ConnectionConfig, ConnectorError,
    ConnectorResult, DdlPlan, DescribeTableDefinitionRequest, DescribeTableRequest,
    DistinctColumnValuesRequest, EngineCapabilities, EngineDescriptor, ExecuteQueryRequest,
    ExplainPlan, ExplainQueryRequest, ExportFormat, ExportObjectRequest, ExportObjectResult,
    FetchLargeValueRequest, FetchLargeValueResult, FetchServerMetricsRequest,
    FetchTablePreviewRequest, FetchTablePreviewStatsRequest, FormatConnectionUrlRequest,
    FormattedConnectionUrl, ImportObjectRequest, ImportObjectResult, IpcError, IpcErrorKind,
    IpcPrompt, ListCatalogChildrenRequest, ListCatalogColumnsRequest, ListCatalogRequest,
    ListRelationshipsRequest, ListTableDefinitionsRequest, ObjectStructure, ObjectStructureRequest,
    ParseConnectionUrlRequest, ParsedConnectionUrl, PinStatus, PlanDdlRequest,
    PlanSchemaDiffRequest, PoolConfig, PoolStatus, PreviewImportRequest, PreviewImportResult,
    ProgressPump, QueryBatchResult, QueryColumn, QueryMode, QueryResult, RenamePair,
    RollbackTableEditsRequest, RowProgress, SchemaChange, SchemaMigration, ServerMetricsSnapshot,
    SessionInfo, SshHostKeyPrompt, StructureSection, TableAdminOp, TableAdminRequest,
    TableAdminResult, TableCountOutcome, TableDefinition, TableEditColumn, TablePreviewColumnRef,
    TablePreviewCount, TablePreviewResult, TablePreviewStats, TableRowInsert, TableRowUpdate,
    TerminateSessionRequest, TerminateSessionResult, TestResult, TransferProgress,
};
