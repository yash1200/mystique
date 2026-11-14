//! Query DTOs shared by plugins and core (execute path).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::cancel::QueryControl;

/// Read vs write. Connection profile `read_only` forces Read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum QueryMode {
    #[default]
    Read,
    Write,
}

/// API / IPC request to run a statement against an open session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ExecuteQueryRequest {
    pub session_id: String,
    pub text: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub mode: QueryMode,
    /// Cap on returned rows. `u32::MAX` (the query canvas) fetches every row.
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
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub params: Vec<JsonValue>,
}

/// One result column.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
    /// The engine's own shape for a result that is not a table (a Redis
    /// value, search hits). `columns`/`rows` still hold a tabular form of
    /// it, so export, history, MCP and the agent work unchanged; the UI
    /// shows this richer view when it has one for the kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub payload: Option<QueryPayload>,
}

/// A non-tabular result. See [`QueryResult::payload`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum QueryPayload {
    /// One value: a string, hash, list, set, sorted set, stream or document.
    Value {
        value: JsonValue,
        /// Engine type label (`string`, `hash`, `zset`, `ReJSON-RL`).
        data_type: String,
        /// Time to live in milliseconds; `None` when the key does not expire.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        ttl_ms: Option<i64>,
        /// `value` was cut to fit the IPC budget.
        #[serde(default)]
        truncated: bool,
    },
    /// Search hits, best first.
    Hits {
        total: u64,
        /// False when the engine reports a lower bound (`gte`).
        total_exact: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        max_score: Option<f64>,
        hits: Vec<SearchHit>,
        /// Hits were dropped to fit the IPC budget.
        #[serde(default)]
        truncated: bool,
    },
    /// A command with no rows (`OK`, `acknowledged: true`).
    Message { text: String },
}

/// One search hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub index: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub score: Option<f64>,
    pub source: JsonValue,
}

/// Largest payload sent over IPC. A bigger value is cut to a preview.
pub const PAYLOAD_MAX_BYTES: usize = 1024 * 1024;
/// Characters kept when a value is cut.
pub const PAYLOAD_PREVIEW_CHARS: usize = 64 * 1024;

fn column(name: &str, data_type: &str) -> QueryColumn {
    QueryColumn {
        name: name.into(),
        data_type: Some(data_type.into()),
    }
}

impl QueryResult {
    /// A result carrying `payload`, with the tabular form filled in from it:
    /// one `value` cell; one row per hit (`_index`, `_id`, `_score`,
    /// `_source`); or no rows and the text as the message.
    pub fn from_payload(
        engine: impl Into<String>,
        mode: QueryMode,
        payload: QueryPayload,
        duration_ms: u64,
    ) -> Self {
        let (columns, rows, message) = match &payload {
            QueryPayload::Value {
                value, data_type, ..
            } => (
                vec![column("value", data_type)],
                vec![vec![value.clone()]],
                None,
            ),
            QueryPayload::Hits { hits, .. } => (
                vec![
                    column("_index", "keyword"),
                    column("_id", "keyword"),
                    column("_score", "float"),
                    column("_source", "object"),
                ],
                hits.iter()
                    .map(|h| {
                        vec![
                            JsonValue::String(h.index.clone()),
                            JsonValue::String(h.id.clone()),
                            h.score
                                .and_then(serde_json::Number::from_f64)
                                .map_or(JsonValue::Null, JsonValue::Number),
                            h.source.clone(),
                        ]
                    })
                    .collect(),
                None,
            ),
            QueryPayload::Message { text } => (Vec::new(), Vec::new(), Some(text.clone())),
        };
        Self {
            engine: engine.into(),
            mode,
            columns,
            row_count: rows.len() as u64,
            rows,
            truncated: false,
            duration_ms,
            message,
            payload: Some(payload),
        }
    }
}

/// Keep a payload under [`PAYLOAD_MAX_BYTES`]: a value becomes a string
/// preview, hits are dropped from the end. Marks it truncated.
pub fn clip_payload(payload: &mut QueryPayload) {
    let size = |p: &QueryPayload| serde_json::to_vec(p).map_or(0, |v| v.len());
    if size(payload) <= PAYLOAD_MAX_BYTES {
        return;
    }
    match payload {
        QueryPayload::Value {
            value, truncated, ..
        } => {
            let text = match &mut *value {
                JsonValue::String(s) => std::mem::take(s),
                other => other.to_string(),
            };
            *value = JsonValue::String(text.chars().take(PAYLOAD_PREVIEW_CHARS).collect());
            *truncated = true;
        }
        QueryPayload::Hits {
            hits, truncated, ..
        } => {
            // Halve until it fits; a single huge hit keeps a preview source.
            while hits.len() > 1
                && serde_json::to_vec(&*hits).map_or(0, |v| v.len()) > PAYLOAD_MAX_BYTES
            {
                hits.truncate(hits.len() / 2);
            }
            if let Some(hit) = hits.first_mut() {
                if serde_json::to_vec(&hit.source).map_or(0, |v| v.len()) > PAYLOAD_MAX_BYTES {
                    let text = hit.source.to_string();
                    hit.source =
                        JsonValue::String(text.chars().take(PAYLOAD_PREVIEW_CHARS).collect());
                }
            }
            *truncated = true;
        }
        QueryPayload::Message { text } => {
            *text = text.chars().take(PAYLOAD_PREVIEW_CHARS).collect();
        }
    }
}

/// Defaults applied by core when client omits limits.
pub const DEFAULT_MAX_ROWS: u32 = 500;
/// Ceiling for grid previews and other capped fetches. The query canvas is not capped.
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
    fn a_value_payload_has_a_one_cell_table() {
        let result = QueryResult::from_payload(
            "redis",
            QueryMode::Read,
            QueryPayload::Value {
                value: serde_json::json!({"name": "ada"}),
                data_type: "hash".into(),
                ttl_ms: Some(5_000),
                truncated: false,
            },
            3,
        );
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0].name, "value");
        assert_eq!(result.rows, vec![vec![serde_json::json!({"name": "ada"})]]);
        assert_eq!(result.row_count, 1);
        let json = serde_json::to_value(&result).expect("json");
        assert_eq!(json["payload"]["kind"], "value");
        assert_eq!(json["payload"]["dataType"], "hash");
        assert_eq!(json["payload"]["ttlMs"], 5_000);
    }

    #[test]
    fn hits_become_rows_and_a_message_has_none() {
        let hits = QueryResult::from_payload(
            "elasticsearch",
            QueryMode::Read,
            QueryPayload::Hits {
                total: 10,
                total_exact: false,
                max_score: Some(1.5),
                hits: vec![SearchHit {
                    index: "logs".into(),
                    id: "1".into(),
                    score: Some(1.5),
                    source: serde_json::json!({"msg": "hi"}),
                }],
                truncated: false,
            },
            1,
        );
        let names: Vec<_> = hits.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["_index", "_id", "_score", "_source"]);
        assert_eq!(hits.rows[0][2], serde_json::json!(1.5));

        let ok = QueryResult::from_payload(
            "redis",
            QueryMode::Write,
            QueryPayload::Message { text: "OK".into() },
            0,
        );
        assert!(ok.rows.is_empty());
        assert_eq!(ok.message.as_deref(), Some("OK"));
    }

    #[test]
    fn a_huge_value_is_cut_to_a_preview() {
        let mut payload = QueryPayload::Value {
            value: JsonValue::String("x".repeat(PAYLOAD_MAX_BYTES * 2)),
            data_type: "string".into(),
            ttl_ms: None,
            truncated: false,
        };
        clip_payload(&mut payload);
        let QueryPayload::Value {
            value, truncated, ..
        } = payload
        else {
            panic!("still a value");
        };
        assert!(truncated);
        assert_eq!(value.as_str().map(str::len), Some(PAYLOAD_PREVIEW_CHARS));
    }

    #[test]
    fn too_many_hits_are_dropped_from_the_end() {
        let hit = |i: usize| SearchHit {
            index: "i".into(),
            id: i.to_string(),
            score: None,
            source: JsonValue::String("y".repeat(64 * 1024)),
        };
        let mut payload = QueryPayload::Hits {
            total: 100,
            total_exact: true,
            max_score: None,
            hits: (0..100).map(hit).collect(),
            truncated: false,
        };
        clip_payload(&mut payload);
        let QueryPayload::Hits {
            hits, truncated, ..
        } = payload
        else {
            panic!("still hits");
        };
        assert!(truncated);
        assert!(!hits.is_empty() && hits.len() < 100);
        assert_eq!(hits[0].id, "0");
    }

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
