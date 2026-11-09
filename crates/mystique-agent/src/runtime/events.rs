//! What a running turn streams to the window.
//!
//! One tagged union, sent over a Tauri `Channel` and exported to TypeScript,
//! so a later ACP adapter can reuse it unchanged. Every run ends with exactly
//! one `done` or `error`. Deltas arrive batched (see `stream.rs`).

use mystique_core::redact::{redact_json, redact_secrets, redact_sql_passwords};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Token counts the provider reported for the whole run (all model calls).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AgentTokens {
    pub input: u64,
    pub output: u64,
    /// Input tokens served from the provider's prompt cache.
    pub cached_input: u64,
}

impl AgentTokens {
    pub(crate) fn add(&mut self, usage: &rig::completion::Usage) {
        self.input = self.input.saturating_add(usage.input_tokens);
        self.output = self.output.saturating_add(usage.output_tokens);
        self.cached_input = self.cached_input.saturating_add(usage.cached_input_tokens);
    }
}

/// Why a run ended without an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum AgentErrorKind {
    /// The user pressed Stop.
    Cancelled,
    /// The run hit its time limit and was aborted.
    Timeout,
    /// The provider failed or returned nothing usable.
    Provider,
    /// Mystique failed (storage, a panicked task).
    Internal,
}

/// One streamed event of an agent run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentEvent {
    /// Assistant text, batched every 50–100 ms.
    TextDelta { text: String },
    /// Model reasoning ("thinking"), batched like text.
    ReasoningDelta { text: String },
    /// A tool call is about to run. `args` are redacted.
    ToolCallStart {
        id: String,
        name: String,
        #[cfg_attr(feature = "ts", ts(type = "unknown"))]
        args: JsonValue,
    },
    /// A tool call finished. The full result is on the persisted tool message.
    #[serde(rename_all = "camelCase")]
    ToolCallEnd {
        id: String,
        ok: bool,
        summary: String,
        /// The tool card row in local SQLite.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        message_id: Option<String>,
    },
    /// The tool call `id` waits in the approval dock (Phase F). Approving or
    /// denying happens there; this only says why the run is paused.
    #[serde(rename_all = "camelCase")]
    ToolApprovalPending {
        id: String,
        approval_id: String,
        sql: String,
        /// Access level the statement needs (`write` / `full`).
        needed: String,
    },
    /// Older context was dropped (or tool results shortened) to fit the
    /// model's budget. Tool-call / tool-result pairs are dropped together.
    #[serde(rename_all = "camelCase")]
    ContextCompacted {
        dropped_messages: u32,
        trimmed_results: u32,
    },
    /// The turn budget ran out before a final answer. The UI offers Continue.
    Exhausted { turns: u32 },
    /// The assistant reply is saved.
    #[serde(rename_all = "camelCase")]
    Done {
        message_id: String,
        tokens: AgentTokens,
    },
    /// The run ended without an answer.
    Error {
        kind: AgentErrorKind,
        message: String,
    },
}

impl AgentEvent {
    /// The one place the streamed trace is masked: URI userinfo, secret
    /// `key=value` / JSON fields, and account passwords in SQL.
    pub(crate) fn redacted(self) -> Self {
        match self {
            Self::ToolCallStart { id, name, args } => Self::ToolCallStart {
                id,
                name,
                args: redact_args(&args),
            },
            Self::ToolCallEnd {
                id,
                ok,
                summary,
                message_id,
            } => Self::ToolCallEnd {
                id,
                ok,
                summary: redact_text(&summary),
                message_id,
            },
            Self::ToolApprovalPending {
                id,
                approval_id,
                sql,
                needed,
            } => Self::ToolApprovalPending {
                id,
                approval_id,
                sql: redact_text(&sql),
                needed,
            },
            Self::Error { kind, message } => Self::Error {
                kind,
                message: redact_text(&message),
            },
            other => other,
        }
    }
}

/// Tool arguments as they may appear in a trace, a log line, or a tool card.
pub fn redact_args(args: &JsonValue) -> JsonValue {
    match redact_json(args) {
        JsonValue::Object(mut map) => {
            for value in map.values_mut() {
                if let JsonValue::String(s) = value {
                    *s = redact_sql_passwords(s);
                }
            }
            JsonValue::Object(map)
        }
        other => other,
    }
}

pub fn redact_text(text: &str) -> String {
    redact_sql_passwords(&redact_secrets(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn events_serialize_as_a_tagged_union() {
        let ev = AgentEvent::ToolCallEnd {
            id: "c1".into(),
            ok: true,
            summary: "3 rows".into(),
            message_id: Some("9".into()),
        };
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["type"], "toolCallEnd");
        assert_eq!(v["messageId"], "9");
        let done = serde_json::to_value(AgentEvent::Done {
            message_id: "1".into(),
            tokens: AgentTokens::default(),
        })
        .unwrap();
        assert_eq!(done["type"], "done");
        assert_eq!(done["tokens"]["cachedInput"], 0);
    }

    #[test]
    fn redaction_masks_tool_args_and_errors() {
        let ev = AgentEvent::ToolCallStart {
            id: "c1".into(),
            name: "execute_readonly_query".into(),
            args: json!({
                "query": "ALTER USER bob PASSWORD 'hunter2'",
                "password": "s3cret",
                "dsn": "postgres://bob:pa55@db.internal/app",
            }),
        }
        .redacted();
        let text = serde_json::to_string(&ev).unwrap();
        assert!(!text.contains("hunter2"), "{text}");
        assert!(!text.contains("s3cret"), "{text}");
        assert!(!text.contains("pa55"), "{text}");

        let err = AgentEvent::Error {
            kind: AgentErrorKind::Provider,
            message: "could not reach mysql://root:toor@10.0.0.1/x".into(),
        }
        .redacted();
        assert!(!serde_json::to_string(&err).unwrap().contains("toor"));
    }
}
