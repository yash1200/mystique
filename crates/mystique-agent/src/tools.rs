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
            .call_tool(name, &args, ToolCaller::Agent)
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
}
