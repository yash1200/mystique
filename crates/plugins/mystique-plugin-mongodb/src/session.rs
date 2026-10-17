//! MongoDB session — thin [`ConnectorSession`] facade over focused modules.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use mongodb::bson::doc;
use mongodb::Client;
use mystique_connector_api::{
    ApplyDdlResult, ApplyTableEditsInput, ApplyTableEditsResult, CatalogColumnRow, CatalogDatabase,
    ColumnInfo, ConnectorError, ConnectorResult, ConnectorSession, DdlPlan, EngineCapabilities,
    ExplainPlan, ExportObjectInput, ExportObjectResult, FetchLargeValueInput,
    FetchLargeValueResult, FetchServerMetricsInput, FetchTablePreviewInput, ImportChunkInput,
    ImportChunkResult, ImportObjectInput, ImportObjectResult, ObjectStructure, PinnedSession,
    PoolStatus, PreviewImportInput, PreviewImportResult, QueryResult, ServerMetricsSnapshot,
    TableAdminInput, TableAdminResult, TableDefinition, TablePreviewResult, TablePreviewStats,
    TerminateSessionInput, TerminateSessionResult, ValidatedQuery,
};

mod admin;
mod catalog;
mod ddl;
mod edits;
mod encode;
mod error;
mod explain;
mod large;
mod metrics;
mod pin;
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
    /// Set from `hello` at connect. Standalone stays false.
    pub(super) transactions: AtomicBool,
}

impl MongoSession {
    pub fn new(client: Client, max_size: u32, default_db: Option<String>) -> Self {
        Self {
            client,
            max_size,
            default_db,
            closed: AtomicBool::new(false),
            transactions: AtomicBool::new(false),
        }
    }

    pub(super) fn ensure_open(&self) -> ConnectorResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        Ok(())
    }

    pub(crate) async fn note_transactions(&self) {
        if pin::probe_transactions(&self.client).await {
            self.transactions.store(true, Ordering::SeqCst);
        }
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
        let mut caps = EngineCapabilities::mongodb_defaults();
        caps.supports_transactions = self.transactions.load(Ordering::SeqCst);
        caps
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
        // Do not `Client::shutdown()` here. That waits until every clone of the
        // client is dropped; this session still holds one, so shutdown never
        // returns and the connection manager's session lock stays taken —
        // later connects (e.g. SQLite after leaving a Mongo workspace) hang
        // on "Connecting…". Dropping the client on session drop is enough.
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn execute_query(&self, query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        query::execute_query(self, query).await
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
        if !self.transactions.load(Ordering::SeqCst) {
            return Err(ConnectorError::NotSupported(
                "MongoDB transactions need a replica set.".into(),
            ));
        }
        let pin = pin::MongoPin::open(self.share_client(), self.share_default_db()).await?;
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

    async fn fetch_large_value(
        &self,
        input: &FetchLargeValueInput,
    ) -> ConnectorResult<FetchLargeValueResult> {
        large::fetch_large_value(self, input).await
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

#[cfg(test)]
mod live_tests;
