mod defs;
mod leaked;

use mystique_core::{AgentTools, ToolCaller};
use serde_json::{json, Value as JsonValue};
use thiserror::Error;

pub use defs::{DescribeObject, ExecuteReadonly, GetSchemaDbml, ListObjects, ListSavedQueries};
pub use leaked::{extract_leaked_tool_calls, strip_leaked_tool_calls, LeakedToolCall};

pub(super) const ALLOWED: &[&str] = &[
    "get_schema_dbml",
    "list_objects",
    "describe_object",
    "execute_readonly_query",
    "list_saved_queries",
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

    pub(crate) async fn dispatch(&self, name: &str, args: JsonValue) -> Result<JsonValue, ToolErr> {
        if !ALLOWED.contains(&name) {
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

fn redact(value: JsonValue) -> JsonValue {
    let s = value.to_string();
    if s.len() <= 32_768 {
        return value;
    }
    json!({
        "_truncated": true,
        "preview": &s[..32_000],
    })
}
