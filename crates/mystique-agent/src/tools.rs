mod defs;
mod leaked;

use mystique_core::{AgentTools, ToolCaller};
use rig::agent::{AgentBuilder, WithBuilderTools};
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
}

impl BoundTools {
    pub fn new(tools: AgentTools, connection_id: String, database: String) -> Self {
        Self {
            tools,
            connection_id,
            database,
        }
    }

    /// Same rule as MCP's tool list: the write tool only when the agent's
    /// level allows writes somewhere or a connection asks for approval.
    pub fn may_write(&self) -> bool {
        self.tools.may_write(ToolCaller::Agent)
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

    /// Run one tool through the shared [`AgentTools`] path, as the agent.
    /// Levels, production rules and the approval dock apply exactly as for
    /// MCP; a write above the level is denied or waits in the dock.
    pub(crate) async fn dispatch(&self, name: &str, args: JsonValue) -> Result<JsonValue, ToolErr> {
        if !ALLOWED.contains(&name) || (name == WRITE_TOOL && !self.may_write()) {
            return Err(ToolErr(format!("unknown or disabled tool `{name}`")));
        }
        let args = self.bind(args)?;
        self.tools
            .call_tool(name, &args, ToolCaller::Agent)
            .await
            .map(redact)
            .map_err(|e| ToolErr(e.to_string()))
    }
}

/// Register the agent's tools on a Rig builder: every read tool, plus
/// `execute_query` when [`BoundTools::may_write`].
pub fn attach_tools(builder: AgentBuilder, bound: BoundTools) -> AgentBuilder<WithBuilderTools> {
    let may_write = bound.may_write();
    let builder = builder
        .tool(GetSchemaDbml(bound.clone()))
        .tool(ListObjects(bound.clone()))
        .tool(DescribeObject(bound.clone()))
        .tool(SampleRows(bound.clone()))
        .tool(ExplainQuery(bound.clone()))
        .tool(InferFields(bound.clone()))
        .tool(ExecuteReadonly(bound.clone()))
        .tool(ListSavedQueries(bound.clone()));
    if may_write {
        builder.tool(ExecuteQuery(bound))
    } else {
        builder
    }
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
