use mystique_core::{tool_specs, AgentToolSpec};
use rig::completion::ToolDefinition;
use rig::message::ToolName;
use serde_json::{json, Value as JsonValue};

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

/// The definition the model is offered for an in-app tool: the shared
/// spec's name, description and schema, with `connection_id` bound. Our own
/// loop runs the call (`BoundTools::dispatch`), so no Rig `Tool` is needed.
pub(super) fn definition(name: &str) -> Option<ToolDefinition> {
    let spec = shared_tool(name);
    let name = ToolName::new(spec.name).ok()?;
    Some(ToolDefinition::new(
        name,
        spec.description,
        bound_schema(spec.input_schema),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_core::AGENT_MAX_CELL_CHARS;

    #[test]
    fn a_definition_is_the_shared_spec_without_connection_id() {
        let def = definition("describe_object").expect("definition");
        assert_eq!(def.name.as_str(), "describe_object");
        assert_eq!(def.description, shared_tool("describe_object").description);
        assert!(def.parameters["properties"].get("connection_id").is_none());
    }

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
