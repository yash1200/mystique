//! Query DTOs shared by plugins and core (execute path).

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Read vs write. Connection profile `read_only` forces Read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum QueryMode {
    #[default]
    Read,
    Write,
}

/// API / IPC request to run a statement against an open session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteQueryRequest {
    pub session_id: String,
    pub text: String,
    #[serde(default)]
    pub mode: QueryMode,
    /// Soft cap on returned rows (hard-capped in core).
    #[serde(default)]
    pub max_rows: Option<u32>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// When true, force Read regardless of `mode`.
    #[serde(default)]
    pub profile_read_only: bool,
    /// Typed object name after a DROP/TRUNCATE confirm prompt.
    #[serde(default)]
    pub confirm_object_name: Option<String>,
    /// MongoDB: collection to query (required for document finds).
    #[serde(default)]
    pub collection: Option<String>,
    /// Optional database override (Mongo multi-DB explorer; falls back to session default).
    #[serde(default)]
    pub database: Option<String>,
}

/// One result column.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryColumn {
    pub name: String,
    /// Engine type label (`int8`, `numeric`, `text`).
    ///
    /// Integers outside ±(2^53−1) and every numeric/decimal cell are JSON
    /// strings. This tag is how the grid tells that string from ordinary text
    /// and can right-align it.
    #[serde(default)]
    pub data_type: Option<String>,
}

/// Tabular query result for the FE grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub engine: String,
    pub mode: QueryMode,
    pub columns: Vec<QueryColumn>,
    /// Row-major cells. Large integers and numerics are strings; see [`QueryColumn::data_type`].
    pub rows: Vec<Vec<JsonValue>>,
    pub row_count: u64,
    pub truncated: bool,
    pub duration_ms: u64,
    /// Optional human note (e.g. "validated write — not executed").
    #[serde(default)]
    pub message: Option<String>,
}

/// Defaults applied by core when client omits limits.
pub const DEFAULT_MAX_ROWS: u32 = 500;
/// Hard ceiling for a single query fetch (query canvas uses up to this).
pub const HARD_MAX_ROWS: u32 = 50_000;
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Validated payload handed to the plugin (never skip policy).
#[derive(Debug, Clone)]
pub struct ValidatedQuery {
    pub text: String,
    pub mode: QueryMode,
    pub max_rows: u32,
    pub timeout_ms: u64,
    pub collection: Option<String>,
    pub database: Option<String>,
}
