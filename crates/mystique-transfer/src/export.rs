//! Export writers: one page of rows at a time, appended to a file.

mod write;

use serde::{Deserialize, Serialize};

pub use write::{
    write_csv_header, write_csv_rows, write_io, write_json_array_rows, write_jsonl_rows,
    write_sql_insert, writeln_io,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Csv,
    Json,
    Jsonl,
    Sql,
}

impl ExportFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Sql => "sql",
        }
    }
}

/// A result column as the writers need it. The SPI implements this for its
/// catalog column type.
pub trait ExportColumn {
    fn name(&self) -> &str;
    /// Engine type name; picks the SQL literal form (`TRUE` vs `1`, bare numbers).
    fn data_type(&self) -> &str;
}
