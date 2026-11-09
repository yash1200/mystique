//! # mystique-connector-api
//!
//! Stable plugin contract for Mystique engines.
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

mod batch;
mod cancel;
mod capabilities;
mod catalog;
mod config;
mod connector;
mod ddl;
mod error;
mod explain;
mod form;
mod ipc_error;
mod json_cell;
mod large_cell;
mod metrics;
mod object_structure;
mod origin;
mod pin;
mod policy;
mod pool;
mod products;
mod query;
mod stream;
mod table;
mod tls;
mod types;
mod write_guard;
mod write_policy;

pub use batch::*;
pub use cancel::*;
pub use capabilities::*;
pub use catalog::*;
pub use config::*;
pub use connector::*;
pub use ddl::*;
pub use error::*;
pub use explain::{
    explain_rejection, ExplainPlan, ExplainQueryRequest, PlanField, PlanNode, EXPLAIN_NOT_EXECUTED,
    EXPLAIN_REJECTED,
};
pub use form::*;
pub use ipc_error::{IpcError, IpcErrorKind, IpcPrompt};
pub use json_cell::{json_decimal, json_f64, json_i64, json_u64, JS_MAX_SAFE_INTEGER};
pub use large_cell::{
    cell_display_text, clip_batch_result, clip_query_result, clip_query_result_with,
    clip_table_preview, large_cell_select, page_cell, query_result_has_large, FetchLargeValueInput,
    FetchLargeValueRequest, FetchLargeValueResult, LargeCellKey, LargeCellLocator, LargeCellSelect,
    SqlPlaceholder, LARGE_CELL_BYTES, LARGE_CELL_CHUNK_CHARS, LARGE_CELL_PREVIEW_CHARS,
    LARGE_VALUE_MAX_BYTES,
};
pub use metrics::*;
/// Affected-row preview for the production write dialog.
pub use mystique_sql::dml::{
    insert_row_count, row_preview_plan, RowPreviewAction, RowPreviewPlan, TablePath,
};
/// A batch that runs `USE`, so the session's database is no longer the one it opened with.
pub use mystique_sql::switches_database as sql_switches_database;
/// Output columns of a plain single-table SELECT (SQLite has no driver provenance).
pub use mystique_sql::{single_table_select, ProjectedItem, SingleTableSelect};
pub use object_structure::*;
pub use origin::{ColumnOrigin, KeyColumn, ResolvedColumn};
pub use pin::{opens_transaction, PinStatus, PinnedSession};
pub use policy::*;
pub use pool::*;
pub use products::{
    forces_terminate_off, hidden_schemas, product, retry_writes_override, runtime_id,
    stamp_connect_overlay, EngineProduct, ENGINE_PRODUCTS,
};
pub use query::*;
pub use stream::{
    stream_cancelled, stream_column, CollectSink, RowBatcher, RowSink, RowSource, STREAM_BATCH_ROWS,
};
pub use table::*;
pub use tls::{SslMode, TlsSettings};
pub use types::*;
pub use write_guard::{WriteGuardPrompt, WriteRisk, WRITE_GUARD_CODE};
pub use write_policy::{
    confirmation_for, confirmation_unlocks, Confirmation, Environment, WriteAction, WriteState,
    READ_ONLY_MESSAGE,
};
