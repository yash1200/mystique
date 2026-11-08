//! Catalog / column-metadata DTOs. Plugins own engine-specific typing
//! (e.g. `timestamptz` vs information_schema prose). The FE only displays.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::cancel::QueryControl;

/// Foreign-key (or equivalent) target for a column — plugin-owned navigation.
/// The FE shows a jump control; it does not invent relationships.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub table: String,
    pub column: String,
}

/// One column as described by the engine plugin (developer-facing types).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    /// Engine type label as a developer would write it (`timestamptz`, `varchar(255)`).
    pub data_type: String,
    pub nullable: bool,
    #[serde(default)]
    pub pk: bool,
    /// Enum / set member labels when the engine exposes them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    /// Default expression / value when the engine exposes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    /// When set, the grid may jump to this referenced table/column for a cell value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references: Option<ColumnReference>,
}

/// Request to describe a single relation / collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribeTableRequest {
    pub session_id: String,
    pub table: String,
    /// SQL schema (Postgres) or MySQL database name when qualified that way.
    #[serde(default)]
    pub schema: Option<String>,
    /// Database / catalog name (MySQL schema, Mongo DB, optional elsewhere).
    #[serde(default)]
    pub database: Option<String>,
}

/// Flat column row for ER / bulk catalog (plugin-normalized types).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogColumnRow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub table: String,
    pub column: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default)]
    pub pk: bool,
}

/// Optional scope for bulk column listing.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListCatalogColumnsRequest {
    pub session_id: String,
    /// Limit to one database (MySQL / Mongo).
    #[serde(default)]
    pub database: Option<String>,
    /// Limit to one schema (Postgres).
    #[serde(default)]
    pub schema: Option<String>,
}

/// One grid filter clause. Defined next to its compiler in `mystique_sql::filters`.
pub use mystique_sql::filters::TableFilter;

/// IPC: fetch a page of table/collection rows (plugin-owned SQL / find + order).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchTablePreviewRequest {
    pub session_id: String,
    pub table: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub max_rows: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
    /// Plugin-compiled predicates (AND). Empty = unfiltered.
    #[serde(default)]
    pub filters: Vec<TableFilter>,
    /// Raw code-filter box. Plugins compile JSON `{col: val}` or `col = val` expressions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_filter: Option<String>,
    /// Column to `ORDER BY` / Mongo `sort`. Must match a described column.
    /// Empty / unknown = plugin default (PK / `_id` / `rowid`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_column: Option<String>,
    /// `asc` | `desc`. Ignored when `sort_column` is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_dir: Option<String>,
    /// Same id the query canvas uses, so closing the tab cancels the page load.
    #[serde(default)]
    pub execution_id: Option<String>,
}

/// Plugin input (session already resolved).
#[derive(Debug, Clone)]
pub struct FetchTablePreviewInput {
    pub table: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub max_rows: u32,
    pub offset: u32,
    pub filters: Vec<TableFilter>,
    pub code_filter: Option<String>,
    pub sort_column: Option<String>,
    pub sort_dir: Option<String>,
    /// Replaced by the manager with the registered control before the plugin runs.
    pub control: Arc<QueryControl>,
}

impl FetchTablePreviewRequest {
    pub fn into_input(self, default_max: u32, hard_max: u32) -> FetchTablePreviewInput {
        FetchTablePreviewInput {
            table: self.table,
            schema: self.schema,
            database: self.database,
            max_rows: self.max_rows.unwrap_or(default_max).clamp(1, hard_max),
            offset: self.offset.unwrap_or(0),
            filters: self.filters,
            code_filter: self
                .code_filter
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            sort_column: self.sort_column,
            sort_dir: self.sort_dir,
            control: QueryControl::new(),
        }
    }
}

/// Stable preview payload: columns + rows in plugin-defined order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TablePreviewResult {
    pub engine: String,
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: u64,
    /// Full table size (for pager). Meaningful only when `total_known`.
    #[serde(default)]
    pub total_rows: u64,
    /// False on the fast path (COUNT runs later). Pager must not treat
    /// `total_rows` as authoritative until this is true.
    #[serde(default)]
    pub total_known: bool,
    pub truncated: bool,
    pub duration_ms: u64,
}

/// Deferred COUNT + FK targets. Does not block first paint of a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchTablePreviewStatsRequest {
    pub session_id: String,
    pub table: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub filters: Vec<TableFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_filter: Option<String>,
}

impl FetchTablePreviewStatsRequest {
    pub fn into_input(self) -> FetchTablePreviewInput {
        FetchTablePreviewInput {
            table: self.table,
            schema: self.schema,
            database: self.database,
            max_rows: 1,
            offset: 0,
            filters: self.filters,
            code_filter: self
                .code_filter
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            sort_column: None,
            sort_dir: None,
            control: QueryControl::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TablePreviewColumnRef {
    pub column: String,
    pub references: ColumnReference,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TablePreviewStats {
    pub total_rows: u64,
    #[serde(default)]
    pub total_known: bool,
    #[serde(default)]
    pub references: Vec<TablePreviewColumnRef>,
}

/// One table / view / collection in the explorer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogObject {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// `table` | `view` | `collection`
    pub kind: String,
    #[serde(default)]
    pub row_count: u64,
}

/// One database / catalog in the explorer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDatabase {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
    #[serde(default)]
    pub system: bool,
    pub objects: Vec<CatalogObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListCatalogRequest {
    pub session_id: String,
    #[serde(default)]
    pub preferred_database: Option<String>,
    /// When false, plugins return database names only (no tables/collections).
    #[serde(default = "default_include_objects")]
    pub include_objects: bool,
}

fn default_include_objects() -> bool {
    true
}

/// Declared relationship (FK or equivalent) for live ER.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRelationship {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_schema: Option<String>,
    pub from_table: String,
    pub from_column: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_schema: Option<String>,
    pub to_table: String,
    pub to_column: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListRelationshipsRequest {
    pub session_id: String,
    #[serde(default)]
    pub database: Option<String>,
}

impl mystique_transfer::ExportColumn for ColumnInfo {
    fn name(&self) -> &str {
        &self.name
    }

    fn data_type(&self) -> &str {
        &self.data_type
    }
}
