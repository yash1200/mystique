//! The edit set a grid save sends, and the undo a committed save keeps.
//! Serde shapes are part of the IPC contract; the SPI re-exports them.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

/// Column metadata needed to compile a write (types, PK).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct TableEditColumn {
    pub name: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub data_type: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub nullable: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub pk: bool,
}

/// Update one existing row. `original` is used to locate the row (PKs);
/// `values` is the changed column → new value map.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct TableRowUpdate {
    pub original: BTreeMap<String, JsonValue>,
    pub values: BTreeMap<String, JsonValue>,
}

/// Insert one new row. `values` omits empty cells so defaults can apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct TableRowInsert {
    pub values: BTreeMap<String, JsonValue>,
}

/// Delete one existing row. `original` locates the row (PKs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct TableRowDelete {
    pub original: BTreeMap<String, JsonValue>,
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

/// One saved row read back after the save. `hash` covers every column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RowCheck {
    pub sql: String,
    pub params: Vec<JsonValue>,
    pub hash: String,
}

/// Inverse of one committed grid save. Built by the plugin from its own plan.
/// Rollback refuses when any `checks` hash no longer matches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
