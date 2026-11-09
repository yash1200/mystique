//! Normalised explain tree. Plugins parse engine output; the UI does not.

use serde::{Deserialize, Serialize};

use crate::error::ConnectorError;
use crate::query::QueryMode;

/// One labelled fact an engine actually returned (`index`, `actual time`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlanField {
    pub label: String,
    pub value: String,
}

/// One node in an explain tree.
///
/// `cost` and `rows` are omitted when the engine has no such number.
/// Self cost is `cost − Σ children` and is computed by the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlanNode {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<PlanField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PlanNode>,
}

impl PlanNode {
    pub fn leaf(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rows: None,
            cost: None,
            fields: Vec::new(),
            children: Vec::new(),
        }
    }
}

/// Plugin-normalised plan plus the engine payload for the Raw tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ExplainPlan {
    /// Server payload, unchanged. `raw_format` is `json` or `text`.
    pub raw: String,
    pub raw_format: String,
    pub root: PlanNode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// IPC body for `explain_engine_query`. Separate from [`crate::ExecuteQueryRequest`]
/// so execute call sites do not grow an analyze flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ExplainQueryRequest {
    pub session_id: String,
    pub text: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub analyze: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub mode: QueryMode,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub confirm_object_name: Option<String>,
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub execution_id: Option<String>,
    /// Values for `:name` / `$1` / `?` placeholders, bound like a normal run.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub params: Vec<serde_json::Value>,
}

/// Shown when analyze of a non-read is rejected inside the read-only transaction.
pub const EXPLAIN_REJECTED: &str = "Explain did not change any data. This statement is not a read, so it ran inside a read-only transaction and the engine rejected it.";

/// Shown when the engine returned a plan without executing the statement.
pub const EXPLAIN_NOT_EXECUTED: &str =
    "Explain did not change any data. This statement was not executed.";

/// A failed analyze of a non-read becomes a plan the UI can show.
///
/// Cancel and timeout stay errors so the run button can stop waiting.
pub fn explain_rejection(err: ConnectorError) -> Result<ExplainPlan, ConnectorError> {
    if explain_stop(&err) {
        return Err(err);
    }
    Ok(ExplainPlan {
        raw: err.to_string(),
        raw_format: "text".into(),
        root: PlanNode::leaf("Not executed"),
        warnings: vec![EXPLAIN_REJECTED.to_string()],
    })
}

fn explain_stop(err: &ConnectorError) -> bool {
    match err {
        ConnectorError::Timeout(_) => true,
        ConnectorError::Query(message) => {
            let lower = message.to_ascii_lowercase();
            lower.contains("cancel") || lower.contains("interrupt")
        }
        _ => false,
    }
}
