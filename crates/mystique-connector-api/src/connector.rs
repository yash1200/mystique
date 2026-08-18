use async_trait::async_trait;

use crate::capabilities::EngineCapabilities;
use crate::catalog::{
    CatalogColumnRow, CatalogDatabase, CatalogRelationship, ColumnInfo, FetchTablePreviewInput,
    TablePreviewResult,
};
use crate::config::ConnectionConfig;
use crate::edits::{ApplyTableEditsInput, ApplyTableEditsResult};
use crate::error::ConnectorResult;
use crate::form::ConnectionFormSchema;
use crate::object_structure::ObjectStructure;
use crate::pool::{PoolStatus, TestResult};
use crate::query::{QueryResult, ValidatedQuery};
use crate::table_admin::{TableAdminInput, TableAdminResult};
use crate::table_export::{ExportObjectInput, ExportObjectResult};
use crate::table_import::{
    ImportChunkInput, ImportChunkResult, ImportObjectInput, ImportObjectResult, PreviewImportInput,
    PreviewImportResult,
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
    async fn fetch_table_preview(
        &self,
        _input: &FetchTablePreviewInput,
    ) -> ConnectorResult<TablePreviewResult> {
        Err(crate::ConnectorError::NotSupported(format!(
            "fetch_table_preview not implemented for engine `{}`",
            self.engine_id()
        )))
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
}
