//! File-backed SQLite session — thin [`ConnectorSession`] facade over focused modules.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use mystique_connector_api::{
    ApplyDdlResult, ApplyTableEditsInput, ApplyTableEditsResult, CatalogColumnRow, CatalogDatabase,
    CatalogRelationship, ColumnInfo, ConnectorError, ConnectorResult, ConnectorSession, DdlPlan,
    EngineCapabilities, ExplainPlan, ExportObjectInput, ExportObjectResult,
    FetchServerMetricsInput, FetchTablePreviewInput, ImportChunkInput, ImportChunkResult,
    ImportObjectInput, ImportObjectResult, ObjectStructure, PinnedSession, PoolStatus,
    PreviewImportInput, PreviewImportResult, QueryBatchResult, QueryResult, ServerMetricsSnapshot,
    TableAdminInput, TableAdminResult, TableDefinition, TablePreviewResult, TablePreviewStats,
    TerminateSessionInput, TerminateSessionResult, ValidatedBatch, ValidatedQuery,
};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

mod admin;
mod catalog;
mod ddl;
mod edits;
mod encode;
mod error;
mod explain;
mod interrupt;
mod metrics;
mod pin;
mod preview;
mod query;
mod structure;
mod transfer;

pub(crate) use error::query_failed;

pub struct SqliteSession {
    pool: Pool<SqliteConnectionManager>,
    path: PathBuf,
    max_size: u32,
    closed: AtomicBool,
}

impl SqliteSession {
    pub fn new(pool: Pool<SqliteConnectionManager>, path: PathBuf, max_size: u32) -> Self {
        Self {
            pool,
            path,
            max_size,
            closed: AtomicBool::new(false),
        }
    }

    pub(super) fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub(super) fn ensure_open(&self) -> ConnectorResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        Ok(())
    }

    pub(super) fn pool(&self) -> Pool<SqliteConnectionManager> {
        self.pool.clone()
    }

    pub(super) async fn blocking<T, F>(&self, f: F) -> ConnectorResult<T>
    where
        F: FnOnce() -> ConnectorResult<T> + Send + 'static,
        T: Send + 'static,
    {
        tokio::task::spawn_blocking(f)
            .await
            .map_err(|e| ConnectorError::Internal(e.to_string()))?
    }
}

#[async_trait]
impl ConnectorSession for SqliteSession {
    fn engine_id(&self) -> &str {
        "sqlite"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::sqlite_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        self.ensure_open()?;
        let pool = self.pool();
        self.blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            conn.query_row("SELECT 1", [], |_| Ok(()))
                .map_err(query_failed)
        })
        .await
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        let state = self.pool.state();
        Some(PoolStatus {
            engine: "sqlite".into(),
            max_size: self.max_size,
            active: state.connections.saturating_sub(state.idle_connections),
            idle: state.idle_connections,
            size: state.connections,
        })
    }

    async fn close(&self) -> ConnectorResult<()> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn execute_query(&self, query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        query::execute_query(self, query).await
    }

    async fn execute_batch(&self, batch: &ValidatedBatch) -> ConnectorResult<QueryBatchResult> {
        query::execute_batch(self, batch).await
    }

    async fn explain_query(
        &self,
        query: &ValidatedQuery,
        analyze: bool,
    ) -> ConnectorResult<ExplainPlan> {
        explain::explain_query(self, query, analyze).await
    }

    async fn begin_pinned(&self) -> ConnectorResult<Box<dyn PinnedSession + Send>> {
        self.ensure_open()?;
        let path = self.path();
        let pin = self.blocking(move || pin::SqlitePin::open(&path)).await?;
        Ok(Box::new(pin))
    }

    async fn describe_table(
        &self,
        database: Option<&str>,
        schema: Option<&str>,
        table: &str,
    ) -> ConnectorResult<Vec<ColumnInfo>> {
        catalog::describe_table(self, database, schema, table).await
    }

    async fn list_catalog_columns(
        &self,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogColumnRow>> {
        catalog::list_catalog_columns(self, database, schema).await
    }

    async fn apply_table_edits(
        &self,
        edits: &ApplyTableEditsInput,
    ) -> ConnectorResult<ApplyTableEditsResult> {
        edits::apply_table_edits(self, edits).await
    }

    async fn fetch_table_preview(
        &self,
        input: &FetchTablePreviewInput,
    ) -> ConnectorResult<TablePreviewResult> {
        preview::fetch_table_preview(self, input).await
    }

    async fn fetch_table_preview_stats(
        &self,
        input: &FetchTablePreviewInput,
    ) -> ConnectorResult<TablePreviewStats> {
        preview::fetch_table_preview_stats(self, input).await
    }

    async fn list_catalog(
        &self,
        preferred_database: Option<&str>,
        include_objects: bool,
    ) -> ConnectorResult<Vec<CatalogDatabase>> {
        catalog::list_catalog(self, preferred_database, include_objects).await
    }

    async fn list_relationships(
        &self,
        database: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogRelationship>> {
        catalog::list_relationships(self, database).await
    }

    async fn apply_table_admin(
        &self,
        input: &TableAdminInput,
    ) -> ConnectorResult<TableAdminResult> {
        admin::apply_table_admin(self, input).await
    }

    async fn export_object(
        &self,
        input: &ExportObjectInput,
    ) -> ConnectorResult<ExportObjectResult> {
        transfer::export_object(self, input).await
    }

    async fn preview_import(
        &self,
        input: &PreviewImportInput,
    ) -> ConnectorResult<PreviewImportResult> {
        transfer::preview_import(self, input).await
    }

    async fn import_object(
        &self,
        input: &ImportObjectInput,
    ) -> ConnectorResult<ImportObjectResult> {
        transfer::import_object(self, input).await
    }

    async fn import_chunk(&self, input: &ImportChunkInput) -> ConnectorResult<ImportChunkResult> {
        transfer::import_chunk(self, input).await
    }

    async fn describe_table_definition(
        &self,
        _database: Option<&str>,
        _schema: Option<&str>,
        table: &str,
        _kind: Option<&str>,
    ) -> ConnectorResult<TableDefinition> {
        ddl::describe_table_definition(self, table).await
    }

    async fn list_table_definitions(
        &self,
        _database: Option<&str>,
        _schema: Option<&str>,
    ) -> ConnectorResult<Vec<TableDefinition>> {
        ddl::list_table_definitions(self).await
    }

    async fn apply_ddl(
        &self,
        plan: &DdlPlan,
        draft: &TableDefinition,
    ) -> ConnectorResult<ApplyDdlResult> {
        ddl::apply_ddl(self, plan, draft).await
    }

    async fn describe_object_structure(
        &self,
        database: Option<&str>,
        schema: Option<&str>,
        table: &str,
        kind: Option<&str>,
    ) -> ConnectorResult<ObjectStructure> {
        structure::describe_object_structure(self, database, schema, table, kind).await
    }

    async fn fetch_server_metrics(
        &self,
        input: &FetchServerMetricsInput,
    ) -> ConnectorResult<ServerMetricsSnapshot> {
        metrics::fetch_server_metrics(self, input).await
    }

    async fn terminate_backend_session(
        &self,
        input: &TerminateSessionInput,
    ) -> ConnectorResult<TerminateSessionResult> {
        metrics::terminate_backend_session(self, input).await
    }
}
