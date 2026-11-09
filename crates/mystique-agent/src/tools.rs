mod defs;
mod leaked;

use mystique_core::{AgentTools, ToolCaller};
use rig::completion::ToolDefinition;
use rig::tool::tool_definition;
use serde_json::{json, Value as JsonValue};
use thiserror::Error;

pub use defs::{
    DescribeObject, ExecuteQuery, ExecuteReadonly, ExplainQuery, GetSchemaDbml, InferFields,
    ListObjects, ListSavedQueries, SampleRows,
};
pub use leaked::{extract_leaked_tool_calls, strip_leaked_tool_calls, LeakedToolCall};

/// Read tools: always listed.
pub(super) const READ_TOOLS: &[&str] = &[
    "get_schema_dbml",
    "list_objects",
    "describe_object",
    "sample_rows",
    "explain_query",
    "infer_fields",
    "execute_readonly_query",
    "list_saved_queries",
];

/// The write tool. Listed only when the agent may write somewhere (a level
/// of `write`/`full`, or a connection that asks for approval), as on MCP.
pub(super) const WRITE_TOOL: &str = "execute_query";

/// Every tool name the agent may call (leaked-call parsing uses it too).
pub(super) const ALLOWED: &[&str] = &[
    "get_schema_dbml",
    "list_objects",
    "describe_object",
    "sample_rows",
    "explain_query",
    "infer_fields",
    "execute_readonly_query",
    "list_saved_queries",
    WRITE_TOOL,
];

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ToolErr(pub String);

#[derive(Clone)]
pub struct BoundTools {
    tools: AgentTools,
    connection_id: String,
    database: String,
    caller: ToolCaller,
}

impl BoundTools {
    /// Tools for the built-in agent ([`ToolCaller::Agent`]).
    pub fn new(tools: AgentTools, connection_id: String, database: String) -> Self {
        Self {
            tools,
            connection_id,
            database,
            caller: ToolCaller::Agent,
        }
    }

    /// The same tools under another caller's levels: an external (ACP)
    /// agent's lookups run as [`ToolCaller::Mcp`], like its own tool calls.
    pub fn with_caller(mut self, caller: ToolCaller) -> Self {
        self.caller = caller;
        self
    }

    /// Same rule as MCP's tool list: the write tool only when the agent's
    /// level allows writes somewhere or a connection asks for approval.
    pub fn may_write(&self) -> bool {
        self.tools.may_write(self.caller)
    }

    /// Tool names offered to the model for this turn.
    pub fn listed(&self) -> Vec<&'static str> {
        let mut names = READ_TOOLS.to_vec();
        if self.may_write() {
            names.push(WRITE_TOOL);
        }
        names
    }

    fn bind(&self, mut args: JsonValue) -> Result<JsonValue, ToolErr> {
        let obj = args
            .as_object_mut()
            .ok_or_else(|| ToolErr("tool args must be an object".into()))?;
        if let Some(existing) = obj.get("connection_id").and_then(|v| v.as_str()) {
            if !existing.is_empty() && existing != self.connection_id {
                return Err(ToolErr(
                    "connection_id does not match the active chat connection".into(),
                ));
            }
        }
        obj.insert(
            "connection_id".into(),
            JsonValue::String(self.connection_id.clone()),
        );
        if let Some(existing) = obj.get("database").and_then(|v| v.as_str()) {
            if !existing.is_empty() && existing != self.database && !self.database.is_empty() {
                return Err(ToolErr(
                    "database does not match the active workspace database".into(),
                ));
            }
        }
        if !self.database.is_empty() {
            obj.insert("database".into(), JsonValue::String(self.database.clone()));
        }
        Ok(args)
    }

    /// Run one tool through the shared [`AgentTools`] path, as this caller.
    /// Levels, production rules and the approval dock apply exactly as for
    /// MCP; a write above the level is denied or waits in the dock. An
    /// unlisted `execute_query` (a leaked call) gets the same level denial
    /// as on MCP, not a different message.
    pub(crate) async fn dispatch(&self, name: &str, args: JsonValue) -> Result<JsonValue, ToolErr> {
        if !ALLOWED.contains(&name) {
            return Err(ToolErr(format!("unknown or disabled tool `{name}`")));
        }
        let args = self.bind(args)?;
        self.tools
            .call_tool(name, &args, self.caller)
            .await
            .map(redact)
            .map_err(|e| ToolErr(e.to_string()))
    }

    /// [`Self::dispatch`] for the streaming loop: a query error keeps the
    /// line and column it points at, for the repair prompt.
    pub(crate) async fn dispatch_located(
        &self,
        name: &str,
        args: JsonValue,
    ) -> Result<JsonValue, LocatedToolErr> {
        if !ALLOWED.contains(&name) {
            return Err(LocatedToolErr::plain(format!(
                "unknown or disabled tool `{name}`"
            )));
        }
        let args = self.bind(args).map_err(|e| LocatedToolErr::plain(e.0))?;
        self.tools
            .call_tool(name, &args, self.caller)
            .await
            .map(redact)
            .map_err(|e| {
                let at = match &e {
                    mystique_core::AppError::Connector(inner) => {
                        inner.local_position().map(|p| (p.line, p.column))
                    }
                    _ => None,
                };
                LocatedToolErr {
                    message: e.to_string(),
                    at,
                }
            })
    }
}

/// A failed tool call, with the 1-based line and column of a query error.
#[derive(Debug)]
pub(crate) struct LocatedToolErr {
    pub message: String,
    pub at: Option<(u32, u32)>,
}

impl LocatedToolErr {
    fn plain(message: String) -> Self {
        Self { message, at: None }
    }
}

/// The agent's tool definitions for one turn: every read tool, plus
/// `execute_query` when [`BoundTools::may_write`] (the [`BoundTools::listed`]
/// set). The streaming loop sends these with each model call.
pub fn tool_definitions(bound: &BoundTools) -> Vec<ToolDefinition> {
    let b = || bound.clone();
    bound
        .listed()
        .into_iter()
        .filter_map(|name| {
            Some(match name {
                "get_schema_dbml" => tool_definition(&GetSchemaDbml(b())),
                "list_objects" => tool_definition(&ListObjects(b())),
                "describe_object" => tool_definition(&DescribeObject(b())),
                "sample_rows" => tool_definition(&SampleRows(b())),
                "explain_query" => tool_definition(&ExplainQuery(b())),
                "infer_fields" => tool_definition(&InferFields(b())),
                "execute_readonly_query" => tool_definition(&ExecuteReadonly(b())),
                "list_saved_queries" => tool_definition(&ListSavedQueries(b())),
                WRITE_TOOL => tool_definition(&ExecuteQuery(b())),
                _ => return None,
            })
        })
        .collect()
}

fn redact(value: JsonValue) -> JsonValue {
    let s = value.to_string();
    if s.len() <= 32_768 {
        return value;
    }
    let end = floor_char_boundary(&s, 32_000);
    let preview = s.get(..end).unwrap_or("");
    json!({
        "_truncated": true,
        "preview": format!("{preview}…"),
    })
}

/// Largest index `<= i` that is a char boundary. A byte slice here panics.
fn floor_char_boundary(s: &str, i: usize) -> usize {
    let mut end = i.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}

#[cfg(test)]
mod write_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_truncates_on_a_char_boundary() {
        let value = JsonValue::String("你".repeat(20_000));
        let raw = value.to_string();
        assert!(raw.len() > 32_768);
        assert!(!raw.is_char_boundary(32_000));
        let out = redact(value);
        assert_eq!(out["_truncated"], JsonValue::Bool(true));
        let preview = out["preview"].as_str().expect("preview");
        let body = preview.strip_suffix('…').expect("truncated marker");
        assert!(body.len() <= 32_000);
        assert!(body.is_char_boundary(body.len()));
    }

    #[test]
    fn every_listed_tool_is_a_shared_spec() {
        let shared: Vec<&str> = mystique_core::tool_specs().iter().map(|s| s.name).collect();
        for name in ALLOWED {
            assert!(shared.contains(name), "{name} has no shared spec");
        }
        assert!(READ_TOOLS.iter().all(|name| *name != WRITE_TOOL));
    }
}
