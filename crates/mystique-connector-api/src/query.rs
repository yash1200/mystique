//! Query DTOs shared by plugins and core (execute path).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::cancel::QueryControl;

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
    /// Client id for [`crate::cancel::QueryControl`]. Unknown ids cancel as a no-op.
    /// Omitted ids are generated in core. Not stored on the connection profile.
    #[serde(default)]
    pub execution_id: Option<String>,
    /// Bound values for `:name`, `?`, and `$n`. They are not spliced into `text`.
    #[serde(default)]
    pub params: Vec<JsonValue>,
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
/// Floor. A client cannot ask for a 0 ms timeout.
pub const MIN_TIMEOUT_MS: u64 = 1_000;
/// Global ceiling. The query canvas sends this. Longer values are clamped.
/// There is no per-profile timeout column yet; this cap is process-wide.
pub const MAX_TIMEOUT_MS: u64 = 120_000;

/// `None` and `0` use [`DEFAULT_TIMEOUT_MS`], not "run forever".
/// The agent passes 30s and stays there. The canvas passes [`MAX_TIMEOUT_MS`].
pub fn clamp_timeout_ms(requested: Option<u64>) -> u64 {
    let raw = match requested {
        None | Some(0) => DEFAULT_TIMEOUT_MS,
        Some(ms) => ms,
    };
    raw.clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS)
}

/// Validated payload handed to the plugin (never skip policy).
#[derive(Debug, Clone)]
pub struct ValidatedQuery {
    pub text: String,
    pub mode: QueryMode,
    pub max_rows: u32,
    pub timeout_ms: u64,
    pub collection: Option<String>,
    pub database: Option<String>,
    pub control: Arc<QueryControl>,
    pub params: Vec<JsonValue>,
}

impl ValidatedQuery {
    pub fn new(text: impl Into<String>, mode: QueryMode, max_rows: u32, timeout_ms: u64) -> Self {
        Self {
            text: text.into(),
            mode,
            max_rows,
            timeout_ms,
            collection: None,
            database: None,
            control: QueryControl::new(),
            params: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_and_missing_timeouts_use_the_default_not_forever() {
        assert_eq!(clamp_timeout_ms(None), DEFAULT_TIMEOUT_MS);
        assert_eq!(clamp_timeout_ms(Some(0)), DEFAULT_TIMEOUT_MS);
    }

    #[test]
    fn timeout_is_clamped_to_the_global_window() {
        assert_eq!(clamp_timeout_ms(Some(1)), MIN_TIMEOUT_MS);
        assert_eq!(clamp_timeout_ms(Some(30_000)), 30_000);
        assert_eq!(clamp_timeout_ms(Some(120_000)), MAX_TIMEOUT_MS);
        assert_eq!(clamp_timeout_ms(Some(9_000_000)), MAX_TIMEOUT_MS);
    }
}
