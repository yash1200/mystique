//! Import parsers: a file becomes keys plus rows of JSON cells.

mod parse;
mod sql_dump;

use std::path::Path;

use serde::{Deserialize, Serialize};

pub use parse::{parse_import_file, suggest_map, ParsedTable};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum ImportOnDuplicate {
    StopOnError,
    UpdateOnDuplicate,
}

impl Default for ImportOnDuplicate {
    fn default() -> Self {
        Self::StopOnError
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ImportColumnMap {
    pub file_key: String,
    #[serde(default)]
    pub table_column: Option<String>,
}
