//! Plugin-owned table/collection file import.
//!
//! The FE picks a file + encoding + column map. Plugins write 100-row chunks.
//! File parsing and encodings live in `mystique-transfer`; the formats are
//! re-exported here (shim: remove after one release).

mod run;

pub use mystique_transfer::{ImportColumnMap, ImportEncoding, ImportFormat, ImportOnDuplicate};
pub use run::{import_chunk_sql, import_object_from_file, preview_import_with_describe};

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::catalog::ColumnInfo;

pub const IMPORT_CHUNK_SIZE: usize = 100;
pub const IMPORT_PREVIEW_ROWS: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct PreviewImportRequest {
    pub session_id: String,
    pub name: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub path: String,
    pub format: ImportFormat,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub encoding: ImportEncoding,
}

#[derive(Debug, Clone)]
pub struct PreviewImportInput {
    pub name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub kind: Option<String>,
    pub path: String,
    pub format: ImportFormat,
    pub encoding: ImportEncoding,
}

impl PreviewImportRequest {
    pub fn into_input(self) -> PreviewImportInput {
        PreviewImportInput {
            name: self.name,
            schema: self.schema,
            database: self.database,
            kind: self.kind,
            path: self.path,
            format: self.format,
            encoding: self.encoding,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ImportTableColumn {
    pub name: String,
    pub data_type: String,
    pub pk: bool,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PreviewImportResult {
    pub file_keys: Vec<String>,
    pub columns: Vec<ImportTableColumn>,
    pub suggested_map: Vec<ImportColumnMap>,
    pub rows: Vec<Vec<JsonValue>>,
    pub format: ImportFormat,
    pub encoding: ImportEncoding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ImportObjectRequest {
    pub session_id: String,
    pub name: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub path: String,
    pub format: ImportFormat,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub encoding: ImportEncoding,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub mapping: Vec<ImportColumnMap>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub on_duplicate: ImportOnDuplicate,
}

#[derive(Debug, Clone)]
pub struct ImportObjectInput {
    pub name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub kind: Option<String>,
    pub path: String,
    pub format: ImportFormat,
    pub encoding: ImportEncoding,
    pub mapping: Vec<ImportColumnMap>,
    pub on_duplicate: ImportOnDuplicate,
}

impl ImportObjectRequest {
    pub fn into_input(self) -> ImportObjectInput {
        ImportObjectInput {
            name: self.name,
            schema: self.schema,
            database: self.database,
            kind: self.kind,
            path: self.path,
            format: self.format,
            encoding: self.encoding,
            mapping: self.mapping,
            on_duplicate: self.on_duplicate,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ImportObjectResult {
    pub inserted: u64,
    pub updated: u64,
    pub format: ImportFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImportChunkInput {
    pub name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<BTreeMap<String, JsonValue>>,
    pub on_duplicate: ImportOnDuplicate,
}

#[derive(Debug, Clone, Default)]
pub struct ImportChunkResult {
    pub inserted: u64,
    pub updated: u64,
}
