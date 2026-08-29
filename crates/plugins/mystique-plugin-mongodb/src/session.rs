//! MongoDB session — thin [`ConnectorSession`] facade over focused modules.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use mongodb::bson::doc;
use mongodb::Client;
use mystique_connector_api::{
    ApplyTableEditsInput, ApplyTableEditsResult, CatalogColumnRow, CatalogDatabase, ColumnInfo,
    ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities, ExportObjectInput,
    ExportObjectResult, FetchServerMetricsInput, FetchTablePreviewInput, ImportChunkInput,
    ImportChunkResult, ImportObjectInput, ImportObjectResult, ObjectStructure, PoolStatus,
    PreviewImportInput, PreviewImportResult, QueryResult, ServerMetricsSnapshot, TableAdminInput,
    TableAdminResult, TablePreviewResult, TerminateSessionInput, TerminateSessionResult,
    ValidatedQuery,
};

mod admin;
mod catalog;
mod edits;
mod encode;
mod error;
mod metrics;
mod preview;
mod query;
mod structure;
mod transfer;

pub(super) use error::query_failed;

pub struct MongoSession {
    client: Client,
    max_size: u32,
    default_db: Option<String>,
    closed: AtomicBool,
}

impl MongoSession {
    pub fn new(client: Client, max_size: u32, default_db: Option<String>) -> Self {
        Self {
            client,
            max_size,
            default_db,
            closed: AtomicBool::new(false),
        }
    }

    pub(super) fn ensure_open(&self) -> ConnectorResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        Ok(())
    }

    pub(super) fn resolve_database(&self, explicit: Option<&str>) -> ConnectorResult<String> {
        if let Some(db) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
            return Ok(db.to_string());
        }
        if let Some(db) = self.default_db.as_deref().filter(|s| !s.is_empty()) {
            return Ok(db.to_string());
        }
        Err(ConnectorError::InvalidConfig(
            "No MongoDB database selected. Put `/dbname` in the connection URI, set pluginConfig.database, or pass a database."
                .into(),
        ))
    }
}

#[async_trait]
impl ConnectorSession for MongoSession {
    fn engine_id(&self) -> &str {
        "mongodb"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::mongodb_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        self.ensure_open()?;
        self.client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(|e| ConnectorError::Connect(e.to_string()))?;
        Ok(())
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        Some(PoolStatus {
            engine: "mongodb".into(),
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
            self.client.clone().shutdown().await;
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

#[cfg(test)]
mod live_tests;
