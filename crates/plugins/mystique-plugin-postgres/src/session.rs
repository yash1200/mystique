//! Pooled PostgreSQL session — thin [`ConnectorSession`] facade over focused modules.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use deadpool_postgres::Pool;
use mystique_connector_api::{
    ApplyTableEditsInput, ApplyTableEditsResult, CatalogColumnRow, CatalogDatabase,
    CatalogRelationship, ColumnInfo, ConnectorError, ConnectorResult, ConnectorSession,
    EngineCapabilities, ExportObjectInput, ExportObjectResult, FetchServerMetricsInput,
    FetchTablePreviewInput, ImportChunkInput, ImportChunkResult, ImportObjectInput,
    ImportObjectResult, InterruptGuard, ObjectStructure, PoolStatus, PreviewImportInput,
    PreviewImportResult, QueryBatchResult, QueryControl, QueryResult, ServerMetricsSnapshot,
    TableAdminInput, TableAdminResult, TablePreviewResult, TablePreviewStats,
    TerminateSessionInput, TerminateSessionResult, ValidatedBatch, ValidatedQuery,
};

mod admin;
mod catalog;
mod edits;
mod encode;
mod metrics;
mod preview;
mod query;
mod structure;
mod transfer;

pub(super) use crate::connector::pool::{
    pg_col, pg_i32_col, pg_u32_col, query_failed, PgCancelTls,
};

pub struct PostgresSession {
    pool: Pool,
    max_size: u32,
    closed: AtomicBool,
    /// Cloned into `CancelToken::cancel_query` so SSL cancels use the same TLS.
    cancel_tls: PgCancelTls,
}

impl PostgresSession {
    pub fn new(pool: Pool, max_size: u32, cancel_tls: PgCancelTls) -> Self {
        Self {
            pool,
            max_size,
            closed: AtomicBool::new(false),
            cancel_tls,
        }
    }

    /// `FnOnce` is sync. The cancel connection is spawned so the query task can
    /// keep polling and still `RESET` / `ROLLBACK`.
    pub(super) fn arm_cancel<'a>(
        &self,
        client: &deadpool_postgres::Object,
        control: &'a QueryControl,
    ) -> InterruptGuard<'a> {
        let token = client.cancel_token();
        let tls = self.cancel_tls.clone();
        control.set_interrupt(move || {
            let Ok(handle) = tokio::runtime::Handle::try_current() else {
                return;
            };
            handle.spawn(async move {
                tls.cancel_query(token).await;
            });
        });
        InterruptGuard::new(control)
    }

    pub(super) fn ensure_open(&self) -> ConnectorResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        Ok(())
    }

    pub(super) async fn client(&self) -> ConnectorResult<deadpool_postgres::Object> {
        self.ensure_open()?;
        self.pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(crate::connector::pool::format_error(&e)))
    }
}

#[async_trait]
impl ConnectorSession for PostgresSession {
    fn engine_id(&self) -> &str {
        "postgres"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::postgres_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        let client = self.client().await?;
        client
            .simple_query("SELECT 1")
            .await
            .map_err(query_failed)?;
        Ok(())
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        let status = self.pool.status();
        Some(PoolStatus {
            engine: "postgres".into(),
            max_size: self.max_size,
            active: status.size.saturating_sub(status.available) as u32,
            idle: status.available as u32,
            size: status.size as u32,
        })
    }

    async fn close(&self) -> ConnectorResult<()> {
        if self
            .closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.pool.close();
        }
        Ok(())
    }

    async fn execute_query(&self, query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        query::execute_query(self, query).await
    }

    async fn execute_batch(&self, batch: &ValidatedBatch) -> ConnectorResult<QueryBatchResult> {
        query::execute_batch(self, batch).await
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
