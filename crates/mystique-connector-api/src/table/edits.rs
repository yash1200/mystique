//! Structured table/collection edits. The FE sends the edit set;
//! each plugin compiles and executes native writes.
//!
//! The edit-set types live in `mystique_sql::dml` next to the compiler; the
//! IPC requests, results and SPI errors stay here.

use serde::{Deserialize, Serialize};

pub use mystique_sql::dml::{
    rows_hash, ApplyTableEditsInput, GridUndo, RowCheck, TableEditColumn, TableRowDelete,
    TableRowInsert, TableRowUpdate,
};

use crate::catalog::ColumnInfo;
use crate::error::ConnectorError;

/// IPC request for a grid save. Read-only comes from the live session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ApplyTableEditsRequest {
    pub session_id: String,
    pub table: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub columns: Vec<TableEditColumn>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub updates: Vec<TableRowUpdate>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub inserts: Vec<TableRowInsert>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub deletes: Vec<TableRowDelete>,
    #[serde(default)]
    pub confirm_object_name: Option<String>,
    /// Compile the statements and do not write.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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

pub const KEYLESS_AMBIGUOUS: &str = "keyless_ambiguous";
pub const AFFECTED_MISMATCH: &str = "affected_mismatch";

pub fn keyless_ambiguous(count: i64) -> ConnectorError {
    ConnectorError::Query(format!(
        "{KEYLESS_AMBIGUOUS}: {count} rows match every column. Refusing the change."
    ))
}

pub fn affected_mismatch(kind: &str, count: u64) -> ConnectorError {
    ConnectorError::Query(format!("{AFFECTED_MISMATCH}: {kind} changed {count} rows."))
}

/// Catalog primary keys replace whatever the frontend sent.
pub fn overlay_catalog_pk(columns: &mut Vec<TableEditColumn>, described: &[ColumnInfo]) {
    let pk_names: Vec<&str> = described
        .iter()
        .filter(|column| column.pk)
        .map(|column| column.name.as_str())
        .collect();
    if pk_names.is_empty() {
        for column in columns.iter_mut() {
            column.pk = false;
        }
        return;
    }
    for column in columns.iter_mut() {
        column.pk = pk_names.iter().any(|name| *name == column.name);
    }
    for described in described.iter().filter(|column| column.pk) {
        if columns.iter().any(|column| column.name == described.name) {
            continue;
        }
        columns.push(TableEditColumn {
            name: described.name.clone(),
            data_type: described.data_type.clone(),
            nullable: described.nullable,
            pk: true,
        });
    }
}

pub const ROLLBACK_STALE: &str = "rollback_stale";

pub fn rollback_stale() -> ConnectorError {
    ConnectorError::Query(format!(
        "{ROLLBACK_STALE}: The rows changed after this save. Rollback would overwrite newer data."
    ))
}

fn default_distinct_limit() -> u32 {
    1_000
}

/// Distinct values for one catalog column. `limit` is clamped to 1000.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
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
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub limit: u32,
    /// Only values whose text starts with this (filter-value completion).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub prefix: Option<String>,
}

/// Plugin-facing distinct request.
#[derive(Debug, Clone)]
pub struct DistinctColumnValuesInput {
    pub table: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub column: String,
    pub limit: u32,
    /// Non-empty, at most 200 characters.
    pub prefix: Option<String>,
}

impl From<DistinctColumnValuesRequest> for DistinctColumnValuesInput {
    fn from(req: DistinctColumnValuesRequest) -> Self {
        Self {
            table: req.table,
            schema: req.schema,
            database: req.database,
            column: req.column,
            limit: req.limit.clamp(1, 1_000),
            prefix: req
                .prefix
                .filter(|p| !p.is_empty())
                .map(|p| p.chars().take(200).collect()),
        }
    }
}

/// Roll back one grid save. The backend owns the inverse SQL; the client
/// sends only the id it got back from the save.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RollbackTableEditsRequest {
    pub session_id: String,
    pub save_id: String,
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
}
