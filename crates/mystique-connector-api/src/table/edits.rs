//! Structured table/collection edits. The FE sends the edit set;
//! each plugin compiles and executes native writes.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

/// Column metadata needed to compile a write (types, PK).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableEditColumn {
    pub name: String,
    #[serde(default)]
    pub data_type: String,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub pk: bool,
}

/// Update one existing row. `original` is used to locate the row (PKs);
/// `values` is the changed column → new value map.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRowUpdate {
    pub original: BTreeMap<String, JsonValue>,
    pub values: BTreeMap<String, JsonValue>,
}

/// Insert one new row. `values` omits empty cells so defaults can apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRowInsert {
    pub values: BTreeMap<String, JsonValue>,
}

/// Delete one existing row. `original` locates the row (PKs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRowDelete {
    pub original: BTreeMap<String, JsonValue>,
}

/// IPC request (includes session + read-only flag).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyTableEditsRequest {
    pub session_id: String,
    pub table: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub columns: Vec<TableEditColumn>,
    #[serde(default)]
    pub updates: Vec<TableRowUpdate>,
    #[serde(default)]
    pub inserts: Vec<TableRowInsert>,
    #[serde(default)]
    pub deletes: Vec<TableRowDelete>,
    #[serde(default)]
    pub profile_read_only: bool,
    #[serde(default)]
    pub confirm_object_name: Option<String>,
    /// Compile the statements and do not write.
    #[serde(default)]
    pub dry_run: bool,
}

/// Plugin-facing input (session already resolved).
#[derive(Debug, Clone)]
pub struct ApplyTableEditsInput {
    pub table: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub columns: Vec<TableEditColumn>,
    pub updates: Vec<TableRowUpdate>,
    pub inserts: Vec<TableRowInsert>,
    pub deletes: Vec<TableRowDelete>,
    pub dry_run: bool,
}

impl From<ApplyTableEditsRequest> for ApplyTableEditsInput {
    fn from(req: ApplyTableEditsRequest) -> Self {
        Self {
            table: req.table,
            schema: req.schema,
            database: req.database,
            columns: req.columns,
            updates: req.updates,
            inserts: req.inserts,
            deletes: req.deletes,
            dry_run: req.dry_run,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyTableEditsResult {
    pub updated: u32,
    pub inserted: u32,
    #[serde(default)]
    pub deleted: u32,
    /// Literal SQL the confirm dialog shows. One statement per changed row.
    #[serde(default)]
    pub statements: Vec<String>,
    /// Inverse statements, last write first.
    #[serde(default)]
    pub rollback_sql: Vec<String>,
}

impl ApplyTableEditsResult {
    pub fn empty() -> Self {
        Self {
            updated: 0,
            inserted: 0,
            deleted: 0,
            statements: Vec::new(),
            rollback_sql: Vec::new(),
        }
    }
}

fn default_distinct_limit() -> u32 {
    1_000
}

/// Distinct values for one catalog column. `limit` is clamped to 1000.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistinctColumnValuesRequest {
    pub session_id: String,
    pub table: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    pub column: String,
    #[serde(default = "default_distinct_limit")]
    pub limit: u32,
}

/// Plugin-facing distinct request.
#[derive(Debug, Clone)]
pub struct DistinctColumnValuesInput {
    pub table: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub column: String,
    pub limit: u32,
}

impl From<DistinctColumnValuesRequest> for DistinctColumnValuesInput {
    fn from(req: DistinctColumnValuesRequest) -> Self {
        Self {
            table: req.table,
            schema: req.schema,
            database: req.database,
            column: req.column,
            limit: req.limit.clamp(1, 1_000),
        }
    }
}

/// Run the inverse statements from a previous save.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackTableEditsRequest {
    pub session_id: String,
    pub table: String,
    #[serde(default)]
    pub statements: Vec<String>,
    #[serde(default)]
    pub profile_read_only: bool,
    #[serde(default)]
    pub confirm_object_name: Option<String>,
}
