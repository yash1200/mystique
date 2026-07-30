//! Catalog / column-metadata DTOs. Plugins own engine-specific typing
//! (e.g. `timestamptz` vs information_schema prose). The FE only displays.

use serde::{Deserialize, Serialize};

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
