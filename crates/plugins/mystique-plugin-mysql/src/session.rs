//! Pooled MySQL / MariaDB session — thin [`ConnectorSession`] facade over focused modules.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use mysql_async::prelude::*;
use mysql_async::Pool;
use mystique_connector_api::{
    ApplyTableEditsInput, ApplyTableEditsResult, CatalogColumnRow, CatalogDatabase,
    CatalogRelationship, ColumnInfo, ConnectorError, ConnectorResult, ConnectorSession,
    EngineCapabilities, ExportObjectInput, ExportObjectResult, FetchTablePreviewInput,
    ImportChunkInput, ImportChunkResult, ImportObjectInput, ImportObjectResult, ObjectStructure,
    PoolStatus, PreviewImportInput, PreviewImportResult, QueryResult, TableAdminInput,
    TableAdminResult, TablePreviewResult, ValidatedQuery,
};

mod admin;
mod catalog;
mod edits;
mod encode;
mod preview;
mod query;
mod structure;
mod transfer;

pub struct MysqlSession {
    pool: Pool,
    max_size: u32,
    closed: AtomicBool,
}

impl MysqlSession {
    pub fn new(pool: Pool, max_size: u32) -> Self {
        Self {
            pool,
            max_size,
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
}

#[async_trait]
impl ConnectorSession for MysqlSession {
    fn engine_id(&self) -> &str {
        "mysql"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::mysql_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        let mut conn = self.conn().await?;
        conn.query_drop("SELECT 1")
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
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
            let pool = self.pool.clone();
            let _ = pool.disconnect().await;
        }
        Ok(())
    }

    async fn execute_query(&self, query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        query::execute_query(self, query).await
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
}
