use std::collections::BTreeMap;

use mystique_connector_api::GridUndo;
use serde::{Deserialize, Serialize};

/// Frozen, hidden, ordered, and resized columns for one table on one connection.
/// `table_id` is `database.schema.name`, so the key covers all four parts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct GridColumnLayoutRecord {
    pub connection_id: String,
    pub table_id: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub order: Vec<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub hidden: Vec<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub frozen: Vec<String>,
    /// Widths the user set, by column name, in unscaled CSS pixels.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub widths: BTreeMap<String, u32>,
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
