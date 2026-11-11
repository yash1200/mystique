//! Pooled MySQL / MariaDB session — thin [`ConnectorSession`] facade over focused modules.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use mysql_async::prelude::*;
use mysql_async::Pool;
use mystique_connector_api::{
    ApplyDdlResult, ApplyTableEditsInput, ApplyTableEditsResult, CatalogColumnRow, CatalogDatabase,
    CatalogRelationship, ColumnInfo, ConnectorError, ConnectorResult, ConnectorSession, DdlPlan,
    EngineCapabilities, ExplainPlan, ExportObjectInput, ExportObjectResult,
    FetchServerMetricsInput, FetchTablePreviewInput, ImportChunkInput, ImportChunkResult,
    ImportObjectInput, ImportObjectResult, ObjectStructure, PinnedSession, PoolStatus,
    PreviewImportInput, PreviewImportResult, QueryBatchResult, QueryControl, QueryResult, RowSink,
    RowSource, ServerMetricsSnapshot, TableAdminInput, TableAdminResult, TableDefinition,
    TablePreviewResult, TablePreviewStats, TerminateSessionInput, TerminateSessionResult,
    ValidatedBatch, ValidatedQuery,
};

mod admin;
mod cancel;
mod catalog;
mod ddl;
mod distinct;
mod edits;
mod encode;
mod explain;
mod metrics;
mod pin;
mod preview;
mod query;
mod read_only;
mod stream;
mod structure;
mod transfer;

pub(super) use crate::connector::pool::{query_failed, query_failed_at};
pub(crate) use read_only::ReadOnlyVar;

pub struct MysqlSession {
    pool: Pool,
    /// Grid saves only. `CLIENT_FOUND_ROWS` makes an unchanged row count as 1.
    edit_pool: Pool,
    max_size: u32,
    read_only: ReadOnlyVar,
    /// MariaDB spells analyze `ANALYZE FORMAT=JSON`, not `EXPLAIN ANALYZE`.
    mariadb: bool,
    closed: AtomicBool,
}

impl MysqlSession {
    pub(crate) fn new(
        pool: Pool,
        edit_pool: Pool,
        max_size: u32,
        read_only: ReadOnlyVar,
        mariadb: bool,
    ) -> Self {
        Self {
            pool,
            edit_pool,
            max_size,
            read_only,
            mariadb,
            closed: AtomicBool::new(false),
        }
    }

    pub(super) fn ensure_open(&self) -> ConnectorResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        Ok(())
    }

    pub(super) async fn conn(&self) -> ConnectorResult<mysql_async::Conn> {
        self.ensure_open()?;
        self.pool
            .get_conn()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))
    }

    pub(super) async fn edit_conn(&self) -> ConnectorResult<mysql_async::Conn> {
        self.ensure_open()?;
        self.edit_pool
            .get_conn()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))
    }
}

#[async_trait]
impl ConnectorSession for MysqlSession {
    fn engine_id(&self) -> &str {
        "mysql"
    }

    fn capabilities(&self) -> EngineCapabilities {
        crate::default_capabilities()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        let mut conn = self.conn().await?;
        conn.query_drop("SELECT 1").await.map_err(query_failed)?;
        Ok(())
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        // mysql_async does not expose idle/active counts cheaply; report configured max.
        Some(PoolStatus {
            engine: "mysql".into(),
            max_size: self.max_size,
            active: 0,
            idle: 0,
            size: self.max_size,
        })
    }

    async fn close(&self) -> ConnectorResult<()> {
        if self
            .closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let _ = self.pool.clone().disconnect().await;
            let _ = self.edit_pool.clone().disconnect().await;
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
        let conn = self.conn().await?;
        Ok(Box::new(pin::MysqlPin::new(self.pool(), conn)))
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
        let mut conn = self.conn().await?;
        stream::stream_rows(&self.pool, &mut conn, source, control, sink).await
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
