//! # mystique-transfer
//!
//! File formats for table import and export: CSV / JSON / JSONL / SQL-dump
//! parsers, text encodings, and the export writers.
//!
//! No session or SPI types. `mystique-connector-api` drives these with the
//! pages a plugin returns and maps [`TransferError`] onto its own error.

mod encoding;
mod error;
pub mod export;
pub mod import;

pub use encoding::{decode_file, ImportEncoding};
pub use error::{TransferError, TransferResult};
pub use export::{ExportColumn, ExportFormat};
pub use import::{
    parse_import_file, stream_import_file, suggest_map, ImportColumnMap, ImportFormat,
    ImportOnDuplicate, ImportRecord, ParsedTable,
};
