use mystique_connector_api::GridUndo;
use serde::{Deserialize, Serialize};

/// Frozen, hidden, and ordered columns for one table on one connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GridColumnLayoutRecord {
    pub connection_id: String,
    pub table_id: String,
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default)]
    pub frozen: Vec<String>,
}

/// One committed grid save that can still be rolled back. Only the backend reads `undo`.
#[derive(Debug, Clone, PartialEq)]
pub struct GridSaveRecord {
    pub save_id: String,
    pub connection_id: String,
    pub table_key: String,
    pub undo: GridUndo,
    pub created_at: String,
}

/// What the grid needs to offer rollback. No SQL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GridSaveSummary {
    pub save_id: String,
    pub created_at: String,
}

/// Saves are listed per table: database, schema, and name.
pub fn grid_table_key(database: Option<&str>, schema: Option<&str>, table: &str) -> String {
    let part = |value: Option<&str>| value.map(str::trim).unwrap_or("").to_string();
    serde_json::to_string(&[part(database), part(schema), table.trim().to_string()])
        .unwrap_or_default()
}
