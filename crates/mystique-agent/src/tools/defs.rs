use mystique_core::{tool_specs, AgentToolSpec};
use rig::tool::{Tool, ToolContext};
use serde_json::{json, Value as JsonValue};

use super::{BoundTools, ToolErr};

/// In-app tools use the same spec as MCP. `connection_id` is bound by the chat.
fn shared_tool(name: &str) -> AgentToolSpec {
    tool_specs()
        .into_iter()
        .find(|tool| tool.name == name)
        .unwrap_or_else(|| AgentToolSpec {
            name: "unknown",
            description: "Unknown tool",
            input_schema: json!({ "type": "object", "additionalProperties": false }),
            annotations: Default::default(),
        })
}

pub(super) fn bound_schema(mut schema: JsonValue) -> JsonValue {
    if let Some(props) = schema.get_mut("properties").and_then(|v| v.as_object_mut()) {
        props.remove("connection_id");
    }
    if let Some(required) = schema.get_mut("required").and_then(|v| v.as_array_mut()) {
        required.retain(|item| item.as_str() != Some("connection_id"));
    }
    schema
}

macro_rules! json_tool {
    ($ty:ident, $name:literal) => {
        #[derive(Clone)]
        pub struct $ty(pub BoundTools);

        impl Tool for $ty {
            const NAME: &'static str = $name;
            type Args = JsonValue;
            type Output = JsonValue;
            type Error = ToolErr;

            fn description(&self) -> String {
                shared_tool($name).description.to_string()
            }

            fn parameters(&self) -> JsonValue {
                bound_schema(shared_tool($name).input_schema)
            }

            async fn call(
                &self,
                _ctx: &mut ToolContext,
                args: Self::Args,
            ) -> Result<Self::Output, Self::Error> {
                self.0.dispatch($name, args).await
            }
        }
    };
}

json_tool!(GetSchemaDbml, "get_schema_dbml");
json_tool!(ListObjects, "list_objects");
json_tool!(DescribeObject, "describe_object");
json_tool!(ExecuteReadonly, "execute_readonly_query");
json_tool!(ListSavedQueries, "list_saved_queries");

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_core::AGENT_MAX_CELL_CHARS;

    #[test]
    fn readonly_query_schema_documents_the_cell_window() {
        let tool = shared_tool("execute_readonly_query");
        assert!(tool.description.contains("next_offset"));
        let schema = bound_schema(tool.input_schema);
        assert!(schema
            .get("properties")
            .and_then(|v| v.get("connection_id"))
            .is_none());
        assert_eq!(
            schema
                .get("properties")
                .and_then(|v| v.get("cell_char_limit"))
                .and_then(|v| v.get("maximum"))
                .and_then(|v| v.as_u64()),
            Some(u64::from(AGENT_MAX_CELL_CHARS))
        );
        assert!(schema
            .get("properties")
            .and_then(|v| v.get("cell_char_offset"))
            .is_some());
    }
}
