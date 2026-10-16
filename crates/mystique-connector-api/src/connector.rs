use std::sync::Arc;

use async_trait::async_trait;

use crate::batch::{BatchEvent, BatchPhase, QueryBatchResult, StatementResult, ValidatedBatch};
use crate::capabilities::EngineCapabilities;
use crate::catalog::{
    CatalogColumnRow, CatalogDatabase, CatalogRelationship, ColumnInfo, FetchTablePreviewInput,
    TablePreviewResult, TablePreviewStats,
};
use crate::config::{ConnectionConfig, FormattedConnectionUrl, ParsedConnectionUrl};
use crate::ddl::{ApplyDdlResult, DdlPlan, TableDefinition};
use crate::error::ConnectorResult;
use crate::explain::ExplainPlan;
use crate::form::ConnectionFormSchema;
use crate::metrics::{
    FetchServerMetricsInput, ServerMetricsSnapshot, TerminateSessionInput, TerminateSessionResult,
};
use crate::object_structure::ObjectStructure;
use crate::pin::PinnedSession;
use crate::pool::{PoolStatus, TestResult};
use crate::query::{QueryResult, ValidatedQuery};
use crate::table::{
    ApplyTableEditsInput, ApplyTableEditsResult, ExportObjectInput, ExportObjectResult,
    ImportChunkInput, ImportChunkResult, ImportObjectInput, ImportObjectResult, PreviewImportInput,
    PreviewImportResult, TableAdminInput, TableAdminResult,
};
use crate::types::EngineDescriptor;
use crate::EngineFamily;

/// Factory for one database engine (postgres, sqlite, mongodb, …).
///
/// Implement this in a plugin crate and register it once.
#[async_trait]
pub trait DatabaseConnector: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn family(&self) -> EngineFamily;
    fn description(&self) -> &'static str;

    fn capabilities(&self) -> EngineCapabilities;
    fn connection_form_schema(&self) -> ConnectionFormSchema;

    /// Parse a connection URL into discrete `plugin_config` fields.
    ///
    /// Default: not supported. Network engines (postgres, mysql, …) override
    /// this using their driver’s URL parser so the FE can import URLs without
    /// reimplementing scheme rules.
    fn parse_connection_url(&self, url: &str) -> ConnectorResult<ParsedConnectionUrl> {
        let _ = url;
        Err(crate::ConnectorError::NotSupported(format!(
            "connection URL import is not supported for engine `{}`",
            self.id()
        )))
    }

    /// Format discrete `plugin_config` (+ password) into a connection URL.
    ///
    /// Default: not supported. Inverse of [`DatabaseConnector::parse_connection_url`].
    fn format_connection_url(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<FormattedConnectionUrl> {
        let _ = config;
        Err(crate::ConnectorError::NotSupported(format!(
            "connection URL export is not supported for engine `{}`",
            self.id()
        )))
    }

    fn descriptor(&self) -> EngineDescriptor {
        let caps = self.capabilities();
        EngineDescriptor {
            id: self.id().into(),
            display_name: self.display_name().into(),
            family: self.family(),
            query_language: caps.query_language.clone(),
            available: true,
            supports_pooling: caps.supports_pooling,
            description: self.description().into(),
            capabilities: Some(caps),
        }
    }

    /// Cheap connectivity check (may open a short-lived connection).
    async fn test_connection(&self, config: &ConnectionConfig) -> ConnectorResult<TestResult>;

    /// Open a pooled session for ongoing work.
    async fn connect(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<Box<dyn ConnectorSession>>;
}

/// Live session backed by an engine-specific connection pool.
#[async_trait]
pub trait ConnectorSession: Send + Sync {
    fn engine_id(&self) -> &str;
    fn capabilities(&self) -> EngineCapabilities;

    /// Health check using a pooled connection.
    async fn ping(&self) -> ConnectorResult<()>;

    /// Current pool snapshot (if the engine pools).
    fn pool_status(&self) -> Option<PoolStatus>;

    /// Drain the pool and release resources.
    async fn close(&self) -> ConnectorResult<()>;

    /// Run a policy-validated query. Default: not supported.
    async fn execute_query(&self, _query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        Err(crate::ConnectorError::NotSupported(format!(
            "execute_query not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Explain one statement. `analyze` asks for actual rows.
    ///
    /// The default is unsupported. Plugins normalise their own plan format.
    /// Analyze runs where the statement cannot delete: a read-only transaction
    /// for SQL, and Mongo's explain command (which does not apply writes).
    async fn explain_query(
        &self,
        _query: &ValidatedQuery,
        _analyze: bool,
    ) -> ConnectorResult<ExplainPlan> {
        Err(crate::ConnectorError::NotSupported(format!(
            "explain is not supported for engine `{}`",
            self.engine_id()
        )))
    }

    /// Hold one connection for a query tab.
    ///
    /// The default is unsupported. SQL plugins override this. Mongo does too
    /// when the deployment can start a transaction.
    async fn begin_pinned(&self) -> ConnectorResult<Box<dyn PinnedSession + Send>> {
        Err(crate::ConnectorError::NotSupported(format!(
            "manual transactions are not supported for engine `{}`",
            self.engine_id()
        )))
    }

    /// Run every statement in `batch` on one connection.
    ///
    /// The default walks [`ConnectorSession::execute_query`] once per statement
    /// and keeps earlier results when a later statement fails. SQL plugins
    /// override this. `execute_query` must not call `execute_batch` — this
    /// default would recurse.
    async fn execute_batch(&self, batch: &ValidatedBatch) -> ConnectorResult<QueryBatchResult> {
        let total = u32::try_from(batch.statements.len()).unwrap_or(u32::MAX);
        let mut results = Vec::with_capacity(batch.statements.len());
        for stmt in &batch.statements {
            if let Some(progress) = &batch.progress {
                progress.emit(
                    BatchEvent {
                        index: stmt.index,
                        total,
                        phase: BatchPhase::Started,
                    },
                    false,
                );
            }
            if batch.control.is_cancelled() {
                return Err(crate::ConnectorError::Query("Query cancelled.".into()));
            }
            let one = ValidatedQuery {
                text: stmt.sql.clone(),
                mode: batch.mode,
                max_rows: batch.max_rows,
                timeout_ms: batch.timeout_ms,
                collection: batch.collection.clone(),
                database: batch.database.clone(),
                control: Arc::clone(&batch.control),
            };
            match self.execute_query(&one).await {
                Ok(result) => {
                    results.push(StatementResult::success(stmt, 0, result, None, Vec::new()));
                }
                Err(err) => {
                    if batch.control.is_cancelled() {
                        return Err(err);
                    }
                    results.push(StatementResult::failed(stmt, 0, &err));
                    if let Some(progress) = &batch.progress {
                        progress.emit(
                            BatchEvent {
                                index: stmt.index,
                                total,
                                phase: BatchPhase::Finished,
                            },
                            true,
                        );
                        progress.flush();
                    }
                    return Ok(QueryBatchResult {
                        results,
                        stopped_at: Some(stmt.index),
                    });
                }
            }
            if let Some(progress) = &batch.progress {
                progress.emit(
                    BatchEvent {
                        index: stmt.index,
                        total,
                        phase: BatchPhase::Finished,
                    },
                    true,
                );
            }
        }
        if let Some(progress) = &batch.progress {
            progress.flush();
        }
        Ok(QueryBatchResult {
            results,
            stopped_at: None,
        })
    }

    /// Column metadata for one table/collection with developer-facing types.
    /// Plugins own type labels (`timestamptz`, `varchar(255)`, …) — not the FE.
    async fn describe_table(
        &self,
        _database: Option<&str>,
        _schema: Option<&str>,
        _table: &str,
    ) -> ConnectorResult<Vec<ColumnInfo>> {
        Err(crate::ConnectorError::NotSupported(format!(
            "describe_table not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Bulk column listing for ER / catalog (plugin-normalized types).
    async fn list_catalog_columns(
        &self,
        _database: Option<&str>,
        _schema: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogColumnRow>> {
        Err(crate::ConnectorError::NotSupported(format!(
            "list_catalog_columns not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Persist grid edits (updates + inserts). Plugins own quoting, types, and execution.
    async fn apply_table_edits(
        &self,
        _edits: &ApplyTableEditsInput,
    ) -> ConnectorResult<ApplyTableEditsResult> {
        Err(crate::ConnectorError::NotSupported(format!(
            "apply_table_edits not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Fetch a page of rows. Default order is engine-native (PK / `_id` / `rowid`);
    /// `sort_column` + `sort_dir` override when the column is known.
    ///
    /// Fast path: columns + page only. Do not COUNT or attach FKs here —
    /// those belong on [`ConnectorSession::fetch_table_preview_stats`].
    async fn fetch_table_preview(
        &self,
        _input: &FetchTablePreviewInput,
    ) -> ConnectorResult<TablePreviewResult> {
        Err(crate::ConnectorError::NotSupported(format!(
            "fetch_table_preview not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Background COUNT + FK targets after the page is already on screen.
    async fn fetch_table_preview_stats(
        &self,
        _input: &FetchTablePreviewInput,
    ) -> ConnectorResult<TablePreviewStats> {
        Ok(TablePreviewStats::default())
    }

    /// Explorer tree: databases + tables/collections (plugin-owned listing SQL / APIs).
    /// `include_objects` false = database picker only (skip tables/collections).
    async fn list_catalog(
        &self,
        _preferred_database: Option<&str>,
        _include_objects: bool,
    ) -> ConnectorResult<Vec<CatalogDatabase>> {
        Err(crate::ConnectorError::NotSupported(format!(
            "list_catalog not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Declared relationships for live ER. Default: none (document / KV engines).
    async fn list_relationships(
        &self,
        _database: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogRelationship>> {
        Ok(Vec::new())
    }

    /// Catalog object admin: CREATE text, rename, drop, truncate, duplicate.
    /// Plugins own dialect and execution so new engines do not require FE SQL.
    async fn apply_table_admin(
        &self,
        _input: &TableAdminInput,
    ) -> ConnectorResult<TableAdminResult> {
        Err(crate::ConnectorError::NotSupported(format!(
            "table admin is not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Stream the full table/collection to `input.path` in plugin-owned format.
    /// Implementations must page in [`crate::EXPORT_CHUNK_SIZE`] rows with no row cap.
    async fn export_object(
        &self,
        _input: &ExportObjectInput,
    ) -> ConnectorResult<ExportObjectResult> {
        Err(crate::ConnectorError::NotSupported(format!(
            "export is not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Parse the first 100 file rows and suggest a column map.
    async fn preview_import(
        &self,
        _input: &PreviewImportInput,
    ) -> ConnectorResult<PreviewImportResult> {
        Err(crate::ConnectorError::NotSupported(format!(
            "import is not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Import the whole file in [`crate::IMPORT_CHUNK_SIZE`] write batches.
    async fn import_object(
        &self,
        _input: &ImportObjectInput,
    ) -> ConnectorResult<ImportObjectResult> {
        Err(crate::ConnectorError::NotSupported(format!(
            "import is not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Write one remapped import chunk. Plugins own insert / upsert SQL or Mongo ops.
    async fn import_chunk(&self, _input: &ImportChunkInput) -> ConnectorResult<ImportChunkResult> {
        Err(crate::ConnectorError::NotSupported(format!(
            "import is not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Columns, indexes, and foreign keys the structure editor can change.
    async fn describe_table_definition(
        &self,
        _database: Option<&str>,
        _schema: Option<&str>,
        _table: &str,
        _kind: Option<&str>,
    ) -> ConnectorResult<TableDefinition> {
        Err(crate::ConnectorError::NotSupported(format!(
            "structure editing is not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Every base table in one database or schema, for a same-engine diff.
    async fn list_table_definitions(
        &self,
        _database: Option<&str>,
        _schema: Option<&str>,
    ) -> ConnectorResult<Vec<TableDefinition>> {
        Err(crate::ConnectorError::NotSupported(format!(
            "structure editing is not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Run a plan produced by [`crate::plan_table_alter`]. Statements are not rewritten.
    async fn apply_ddl(
        &self,
        _plan: &DdlPlan,
        _draft: &TableDefinition,
    ) -> ConnectorResult<ApplyDdlResult> {
        Err(crate::ConnectorError::NotSupported(format!(
            "structure editing is not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Columns, indexes, triggers, relations, and other structure details.
    /// Plugins own the sections; the FE only renders them.
    async fn describe_object_structure(
        &self,
        _database: Option<&str>,
        _schema: Option<&str>,
        _table: &str,
        _kind: Option<&str>,
    ) -> ConnectorResult<ObjectStructure> {
        Err(crate::ConnectorError::NotSupported(format!(
            "object structure is not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Server metrics snapshot (overview, live sessions, slow queries).
    /// Plugins own the queries; the FE only renders the contract.
    async fn fetch_server_metrics(
        &self,
        _input: &FetchServerMetricsInput,
    ) -> ConnectorResult<ServerMetricsSnapshot> {
        Err(crate::ConnectorError::NotSupported(format!(
            "server metrics are not implemented for engine `{}`",
            self.engine_id()
        )))
    }

    /// Terminate a backend session (`pg_terminate_backend`, `KILL`, `killOp`).
    async fn terminate_backend_session(
        &self,
        _input: &TerminateSessionInput,
    ) -> ConnectorResult<TerminateSessionResult> {
        Err(crate::ConnectorError::NotSupported(format!(
            "terminating a session is not supported for engine `{}`",
            self.engine_id()
        )))
    }
}
