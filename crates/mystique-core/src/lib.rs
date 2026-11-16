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

/// Internals for `tests/` (feature `test-support`, which only this crate's
/// dev-dependency on itself turns on).
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support {
    pub mod application {
        pub use crate::application::SessionUse;
        use crate::connectors::OpenSessionRequest;
        use crate::error::AppResult;
        use crate::secrets::{Prompt, SecretStore};
        use crate::storage::AppDatabase;

        pub fn bind_with_session(
            db: &AppDatabase,
            secrets: &SecretStore,
            req: OpenSessionRequest,
            prompt: Prompt,
        ) -> AppResult<(OpenSessionRequest, SessionUse)> {
            crate::application::bind_with_session(db, secrets, req, prompt)
        }

        pub fn settle_session_password<T, E: std::fmt::Display>(
            secrets: &SecretStore,
            session: SessionUse,
            result: &Result<T, E>,
        ) {
            crate::application::settle_session_password(secrets, session, result);
        }
    }

    pub mod agent_tools {
        pub use crate::application::agent_tools::types::AgentCellWindow;
    }

    pub mod manager {
        use std::collections::HashMap;
        use std::sync::Arc;
        use std::time::Duration;

        use mystique_connector_api::{ConnectorResult, ConnectorSession, SessionInfo};
        use tokio::sync::RwLock;

        pub use crate::connectors::manager::history::QueryHistoryWriter;
        pub use crate::connectors::manager::{overlay_session_caps, LiveSession, SessionOrigin};
        use crate::connectors::{ConnectionManager, OpenSessionRequest, TimedUnlockStatus};

        /// The live session map, to put test doubles in.
        pub fn sessions(mgr: &ConnectionManager) -> &RwLock<HashMap<String, LiveSession>> {
            &mgr.sessions
        }

        pub fn set_query_history_writer(
            mgr: &ConnectionManager,
            writer: Arc<dyn QueryHistoryWriter>,
        ) {
            mgr.set_query_history_writer(writer);
        }

        pub async fn get_or_open(
            mgr: &ConnectionManager,
            req: OpenSessionRequest,
            origin: SessionOrigin,
        ) -> ConnectorResult<SessionInfo> {
            mgr.get_or_open(req, origin).await
        }

        pub async fn open_session_as(
            mgr: &ConnectionManager,
            req: OpenSessionRequest,
            origin: SessionOrigin,
        ) -> ConnectorResult<SessionInfo> {
            mgr.open_session_as(req, origin).await
        }

        /// A timed write unlock of any length, so tests need not wait minutes.
        pub async fn start_timed_unlock_for(
            mgr: &ConnectionManager,
            session_id: &str,
            length: Duration,
        ) -> ConnectorResult<TimedUnlockStatus> {
            mgr.start_timed_unlock_for(session_id, length).await
        }

        pub async fn session_and_read_only(
            mgr: &ConnectionManager,
            session_id: &str,
        ) -> ConnectorResult<(Arc<dyn ConnectorSession>, bool)> {
            mgr.session_and_read_only(session_id).await
        }
    }

    pub mod secrets {
        use std::sync::Arc;

        pub use crate::secrets::counting::{CountingKeySource, FakeKeychain};
        pub use crate::secrets::session::SessionPasswords;
        use crate::secrets::{KeySource, SecretBackend, SecretStore};

        /// A store over `source`, with `legacy` as the old keychain items.
        pub fn from_parts(
            source: Arc<dyn KeySource>,
            legacy: Option<Arc<FakeKeychain>>,
        ) -> SecretStore {
            SecretStore::from_parts(source, legacy.map(|l| l as Arc<dyn SecretBackend>))
        }

        /// The passwords typed this session.
        pub fn session_passwords(store: &SecretStore) -> &SessionPasswords {
            store.session()
        }

        /// Drop the cached vault key, to prove a path loads nothing.
        pub fn forget_cached(store: &SecretStore) {
            store.forget_cached();
        }
    }
}
