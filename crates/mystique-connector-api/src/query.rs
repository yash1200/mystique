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
/// `timeout_ms` of 0: no statement timeout and no client deadline.
pub const NO_TIMEOUT: u64 = 0;
/// Floor for a timeout that is not [`NO_TIMEOUT`].
pub const MIN_TIMEOUT_MS: u64 = 1_000;
/// Ceiling for a timeout that is not [`NO_TIMEOUT`] (one day).
pub const MAX_TIMEOUT_MS: u64 = 86_400_000;
/// Fixed deadline for grid previews and large-cell fetches.
pub const FETCH_TIMEOUT_MS: u64 = 120_000;

/// `None` uses [`DEFAULT_TIMEOUT_MS`]. `Some(0)` is [`NO_TIMEOUT`], which the
/// canvas sends when the profile or the global setting says "none".
/// The agent and MCP pass 30s.
pub fn clamp_timeout_ms(requested: Option<u64>) -> u64 {
    match requested {
        None => DEFAULT_TIMEOUT_MS,
        Some(NO_TIMEOUT) => NO_TIMEOUT,
        Some(ms) => ms.clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS),
    }
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
    fn missing_timeout_uses_the_default_and_zero_means_none() {
        assert_eq!(clamp_timeout_ms(None), DEFAULT_TIMEOUT_MS);
        assert_eq!(clamp_timeout_ms(Some(0)), NO_TIMEOUT);
    }

    #[test]
    fn timeout_is_clamped_to_the_global_window() {
        assert_eq!(clamp_timeout_ms(Some(1)), MIN_TIMEOUT_MS);
        assert_eq!(clamp_timeout_ms(Some(30_000)), 30_000);
        // Longer than the old 120s cap is kept.
        assert_eq!(clamp_timeout_ms(Some(600_000)), 600_000);
        assert_eq!(clamp_timeout_ms(Some(u64::MAX)), MAX_TIMEOUT_MS);
    }
}
