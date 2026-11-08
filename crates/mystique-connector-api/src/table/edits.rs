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
    /// Backend id for rolling back this save. Set by the manager once the undo is stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_id: Option<String>,
    /// Inverse of a committed save. Stays in the backend.
    #[serde(skip)]
    pub undo: Option<GridUndo>,
}

impl ApplyTableEditsResult {
    pub fn empty() -> Self {
        Self {
            updated: 0,
            inserted: 0,
            deleted: 0,
            statements: Vec::new(),
            save_id: None,
            undo: None,
        }
    }
}

/// One saved row read back after the save. `hash` covers every column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowCheck {
    pub sql: String,
    pub params: Vec<JsonValue>,
    pub hash: String,
}

/// Inverse of one committed grid save. Built by the plugin from its own plan.
/// Rollback refuses when any `checks` hash no longer matches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridUndo {
    pub checks: Vec<RowCheck>,
    /// Inverse statements, last write first.
    pub inverse: Vec<String>,
}

/// Stable hash of rows read for a [`RowCheck`]. Row order does not matter.
pub fn rows_hash(rows: &[Vec<JsonValue>]) -> String {
    let mut encoded: Vec<String> = rows
        .iter()
        .map(|row| serde_json::to_string(row).unwrap_or_default())
        .collect();
    encoded.sort();
    // FNV-1a 64: stable across builds, unlike `DefaultHasher`.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in encoded.len().to_le_bytes().into_iter().chain(
        encoded
            .iter()
            .flat_map(|row| row.bytes().chain(std::iter::once(0xff))),
    ) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub const ROLLBACK_STALE: &str = "rollback_stale";

pub fn rollback_stale() -> crate::ConnectorError {
    crate::ConnectorError::Query(format!(
        "{ROLLBACK_STALE}: The rows changed after this save. Rollback would overwrite newer data."
    ))
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

/// Roll back one grid save. The backend owns the inverse SQL; the client
/// sends only the id it got back from the save.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RollbackTableEditsRequest {
    pub session_id: String,
    pub save_id: String,
    #[serde(default)]
    pub profile_read_only: bool,
    #[serde(default)]
    pub confirm_object_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rollback_request_has_no_sql_field() {
        let ok: Result<RollbackTableEditsRequest, _> =
            serde_json::from_value(json!({ "sessionId": "s", "saveId": "id" }));
        assert!(ok.is_ok());
        let with_sql: Result<RollbackTableEditsRequest, _> = serde_json::from_value(json!({
            "sessionId": "s",
            "saveId": "id",
            "statements": ["DELETE FROM accounts"],
        }));
        assert!(with_sql.is_err());
    }

    #[test]
    fn the_undo_never_reaches_the_frontend() {
        let mut result = ApplyTableEditsResult::empty();
        result.undo = Some(GridUndo {
            checks: Vec::new(),
            inverse: vec!["DELETE FROM t".into()],
        });
        let text = serde_json::to_string(&result).unwrap_or_default();
        assert!(!text.contains("DELETE"), "{text}");
        assert!(!text.contains("rollbackSql"), "{text}");
    }

    #[test]
    fn rows_hash_ignores_order_and_sees_any_change() {
        let a = vec![json!(1), json!("x")];
        let b = vec![json!(2), json!("y")];
        assert_eq!(
            rows_hash(&[a.clone(), b.clone()]),
            rows_hash(&[b.clone(), a.clone()])
        );
        assert_ne!(
            rows_hash(&[a.clone()]),
            rows_hash(&[vec![json!(1), json!("z")]])
        );
        assert_ne!(rows_hash(&[]), rows_hash(&[a]));
    }
}
