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
    pub profile_read_only: bool,
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyTableEditsResult {
    pub updated: u32,
    pub inserted: u32,
}

impl ApplyTableEditsResult {
    pub fn empty() -> Self {
        Self {
            updated: 0,
            inserted: 0,
        }
    }
}
