//! Pooled PostgreSQL session — thin [`ConnectorSession`] facade over focused modules.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use deadpool_postgres::Pool;
use mystique_connector_api::{
    ApplyDdlResult, ApplyTableEditsInput, ApplyTableEditsResult, CatalogColumnRow, CatalogDatabase,
    CatalogRelationship, ColumnInfo, ColumnOrigin, ConnectorError, ConnectorResult,
    ConnectorSession, DdlPlan, EngineCapabilities, ExplainPlan, ExportObjectInput,
    ExportObjectResult, FetchServerMetricsInput, FetchTablePreviewInput, ImportChunkInput,
    ImportChunkResult, ImportObjectInput, ImportObjectResult, ObjectStructure, PinnedSession,
    PoolStatus, PreviewImportInput, PreviewImportResult, QueryBatchResult, QueryControl,
    QueryResult, RowSink, RowSource, ServerMetricsSnapshot, TableAdminInput, TableAdminResult,
    TableDefinition, TablePreviewResult, TablePreviewStats, TerminateSessionInput,
    TerminateSessionResult, ValidatedBatch, ValidatedQuery,
};

mod admin;
mod catalog;
mod ddl;
mod distinct;
mod edits;
mod encode;
mod explain;
mod large;
mod metrics;
mod pin;
mod preview;
mod query;
mod stream;
mod structure;
mod transfer;

pub(super) use crate::connector::notices::{take_notices, NoticeCapture, NoticeHub};
pub(super) use crate::connector::pool::{
    pg_col, pg_i32_col, pg_u32_col, query_failed, query_failed_at, shift_query_position,
    PgCancelTls,
};

pub struct PostgresSession {
    pool: Pool,
    max_size: u32,
    closed: AtomicBool,
    /// Cloned into `CancelToken::cancel_query` so SSL cancels use the same TLS.
    cancel_tls: PgCancelTls,
    notices: NoticeHub,
}

impl PostgresSession {
    pub(crate) fn new(
        pool: Pool,
        max_size: u32,
        cancel_tls: PgCancelTls,
        notices: NoticeHub,
    ) -> Self {
        Self {
            pool,
            max_size,
            closed: AtomicBool::new(false),
            cancel_tls,
            notices,
        }
    }

    pub(super) fn notice_hub(&self) -> &NoticeHub {
        &self.notices
    }

    /// Check out a connection for one cancellable run and arm its interrupt.
    ///
    /// `FnOnce` is sync. The cancel connection is spawned so the query task can
    /// keep polling and still `RESET` / `ROLLBACK`.
    pub(super) async fn run_client<'a>(
        &self,
        control: &'a QueryControl,
    ) -> ConnectorResult<RunClient<'a>> {
        let client = self.client().await?;
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
        Ok(RunClient {
            client: Some(client),
            control,
            discard: false,
        })
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

/// A pooled connection held by one cancellable run.
///
/// On drop the run's interrupt is cleared, then the connection goes back to
/// the pool only if the run was not cancelled (or timed out, which cancels)
/// and did not panic. Otherwise it is closed: the server may still be
/// finishing the statement, and the session state (an open transaction, a
/// `SET`) is unknown. A run that returns early with `?` is covered too.
pub(super) struct RunClient<'a> {
    /// `Some` until `drop`.
    client: Option<deadpool_postgres::Object>,
    control: &'a QueryControl,
    discard: bool,
}

impl RunClient<'_> {
    /// Close the connection when the run ends, whatever the outcome.
    pub(super) fn discard(&mut self) {
        self.discard = true;
    }
}

impl std::ops::Deref for RunClient<'_> {
    type Target = deadpool_postgres::Object;

    fn deref(&self) -> &Self::Target {
        match self.client.as_ref() {
            Some(client) => client,
            // Only `drop` takes the connection out.
            None => unreachable!("RunClient used after drop"),
        }
    }
}

impl Drop for RunClient<'_> {
    fn drop(&mut self) {
        self.control.clear_interrupt();
        let Some(client) = self.client.take() else {
            return;
        };
        if self.discard || self.control.is_cancelled() || std::thread::panicking() {
            drop(deadpool_postgres::Object::take(client));
        }
    }
}

#[async_trait]
impl ConnectorSession for PostgresSession {
    fn engine_id(&self) -> &str {
        "postgres"
    }

    fn capabilities(&self) -> EngineCapabilities {
        crate::default_capabilities()
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

    async fn explain_query(
        &self,
        query: &ValidatedQuery,
        analyze: bool,
    ) -> ConnectorResult<ExplainPlan> {
        explain::explain_query(self, query, analyze).await
    }

    async fn begin_pinned(&self) -> ConnectorResult<Box<dyn PinnedSession + Send>> {
        self.ensure_open()?;
        let client = self.client().await?;
        Ok(Box::new(pin::PostgresPin::new(
            client,
            self.cancel_tls(),
            self.notices.clone(),
        )))
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

    async fn apply_grid_undo(
        &self,
        undo: &mystique_connector_api::GridUndo,
    ) -> ConnectorResult<()> {
        edits::apply_grid_undo(self, undo).await
    }

    async fn distinct_column_values(
        &self,
        input: &mystique_connector_api::DistinctColumnValuesInput,
    ) -> ConnectorResult<Vec<serde_json::Value>> {
        distinct::distinct_column_values(self, input).await
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

    async fn estimate_table_rows(
        &self,
        input: &FetchTablePreviewInput,
    ) -> ConnectorResult<Option<u64>> {
        preview::estimate_table_rows(self, input).await
    }

    async fn stream_rows(
        &self,
        source: &RowSource,
        control: &Arc<QueryControl>,
        sink: &mut dyn RowSink,
    ) -> ConnectorResult<()> {
        self.ensure_open()?;
        let client = self.run_client(control).await?;
        stream::stream_rows(&client, stream::Wrap::Snapshot, source, control, sink).await
    }

    async fn name_column_origins(
        &self,
        origins: Vec<Option<ColumnOrigin>>,
    ) -> ConnectorResult<Vec<Option<ColumnOrigin>>> {
        large::name_column_origins(self, origins).await
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
        database: Option<&str>,
        schema: Option<&str>,
        table: &str,
        _kind: Option<&str>,
    ) -> ConnectorResult<TableDefinition> {
        ddl::describe_table_definition(self, database, schema, table).await
    }

    async fn list_table_definitions(
        &self,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> ConnectorResult<Vec<TableDefinition>> {
        ddl::list_table_definitions(self, database, schema).await
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
