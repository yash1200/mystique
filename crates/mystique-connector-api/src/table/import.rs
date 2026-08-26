//! Plugin-owned table/collection file import.
//!
//! The FE picks a file + encoding + column map. Plugins write 100-row chunks.

mod parse;
mod run;
mod sql_dump;

pub use run::{import_chunk_sql, import_object_from_file, preview_import_with_describe};

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::catalog::ColumnInfo;

pub const IMPORT_CHUNK_SIZE: usize = 100;
pub const IMPORT_PREVIEW_ROWS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportFormat {
    Csv,
    Json,
    Jsonl,
    Sql,
}

impl ImportFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Sql => "sql",
        }
    }

    pub fn from_path(path: &str) -> Option<Self> {
        let ext = Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "csv" => Some(Self::Csv),
            "jsonl" | "ndjson" => Some(Self::Jsonl),
            "json" => Some(Self::Json),
            "sql" => Some(Self::Sql),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Windows1252,
    Iso88591,
}

impl ImportEncoding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
            Self::Windows1252 => "windows-1252",
            Self::Iso88591 => "iso-8859-1",
        }
    }

    fn decoder(self) -> &'static encoding_rs::Encoding {
        match self {
            Self::Utf8 => encoding_rs::UTF_8,
            Self::Utf16Le => encoding_rs::UTF_16LE,
            Self::Utf16Be => encoding_rs::UTF_16BE,
            Self::Windows1252 | Self::Iso88591 => encoding_rs::WINDOWS_1252,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportOnDuplicate {
    StopOnError,
    UpdateOnDuplicate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportColumnMap {
    pub file_key: String,
    #[serde(default)]
    pub table_column: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[serde(rename_all = "camelCase")]
pub struct ImportTableColumn {
    pub name: String,
    pub data_type: String,
    pub pk: bool,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub encoding: ImportEncoding,
    #[serde(default)]
    pub mapping: Vec<ImportColumnMap>,
    #[serde(default)]
    pub on_duplicate: ImportOnDuplicate,
    #[serde(default)]
    pub profile_read_only: bool,
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

impl Default for ImportEncoding {
    fn default() -> Self {
        Self::Utf8
    }
}

impl Default for ImportOnDuplicate {
    fn default() -> Self {
        Self::StopOnError
    }
}
