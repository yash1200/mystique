//! # mystique-connector-api
//!
//! Stable plugin contract for MystiqueDB engines.
//!
//! - **Plugins** implement [`DatabaseConnector`] + [`ConnectorSession`].
//! - **Core** depends only on this crate (never on engine drivers).
//! - Adding a new database = new plugin crate + one registry line.
//!
//! ## Connection pools
//!
//! Each plugin owns its pooling strategy inside the session returned by
//! [`DatabaseConnector::connect`]. Core only tracks sessions via
//! [`PoolStatus`] / session ids.

mod capabilities;
mod catalog;
mod config;
mod connector;
mod edits;
mod error;
mod form;
mod object_structure;
mod policy;
mod pool;
mod query;
mod sql_table_edits;
mod table_admin;
mod table_export;
mod table_filters;
mod table_import;
mod types;

pub use capabilities::*;
pub use catalog::*;
pub use config::*;
pub use connector::*;
pub use edits::*;
pub use error::*;
pub use form::*;
pub use object_structure::*;
pub use policy::*;
pub use pool::*;
pub use query::*;
pub use sql_table_edits::{
    compile_delete, compile_insert, compile_update, compile_upsert, qualify_table, quote_ident,
    sql_literal, SqlIdentStyle,
};
pub use table_admin::*;
pub use table_export::{
    export_object_with_preview, ExportFormat, ExportObjectInput, ExportObjectRequest,
    ExportObjectResult, EXPORT_CHUNK_SIZE,
};
pub use table_filters::{
    mongo_filter_json, parse_code_filter_rules, preview_sort_field, preview_sort_sql,
    sql_preview_where, sql_where_clause,
};
pub use table_import::{
    import_chunk_sql, import_object_from_file, preview_import_with_describe, ImportChunkInput,
    ImportChunkResult, ImportColumnMap, ImportEncoding, ImportFormat, ImportObjectInput,
    ImportObjectRequest, ImportObjectResult, ImportOnDuplicate, ImportTableColumn,
    PreviewImportInput, PreviewImportRequest, PreviewImportResult, IMPORT_CHUNK_SIZE,
    IMPORT_PREVIEW_ROWS,
};
pub use types::*;
