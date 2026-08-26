use mystique_core::{tool_specs, AgentTools};
use serde_json::{json, Value as JsonValue};

pub(super) async fn handle_method(
    tools: &AgentTools,
    method: &str,
    params: JsonValue,
) -> Result<JsonValue, (i64, String)> {
    match method {
        "initialize" => {
            let proto = params
                .get("protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("2024-11-05");
            let version = if proto.starts_with("2025") {
                proto
            } else {
                "2024-11-05"
            };
            Ok(json!({
                "protocolVersion": version,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false }
                },
                "serverInfo": {
                    "name": "mystique",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "instructions": "Mystique is a desktop database studio. Tools are read-only: schema (DBML), list/describe tables or collections, run SELECT/find, save and list queries. Use list_connections first. Never ask the user to paste schema."
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => {
            let tools: Vec<JsonValue> = tool_specs()
                .into_iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema
                    })
                })
                .collect();
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or((-32602, "tools/call requires name".into()))?;
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match tools.call_tool(name, &args).await {
                Ok(value) => Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": value.to_string()
                    }],
                    "structuredContent": value,
                    "isError": false
                })),
                Err(e) => Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": e.to_string()
                    }],
                    "isError": true
                })),
            }
        }
        "resources/list" => Ok(json!({
            "resources": [
                {
                    "uri": "mystique://connections",
                    "name": "Connections",
                    "description": "Saved connections (name, engine, database)",
                    "mimeType": "application/json"
                }
            ]
        })),
        "resources/read" => {
            let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            if uri == "mystique://connections" {
                let list = tools
                    .list_connections()
                    .await
                    .map_err(|e| (-32000, e.to_string()))?;
                let text = serde_json::to_string_pretty(&list).unwrap_or_default();
                return Ok(json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": text
                    }]
                }));
            }
            if let Some(rest) = uri.strip_prefix("mystique://connections/") {
                if let Some(id) = rest.strip_suffix("/schema.dbml") {
                    let dbml = tools
                        .get_schema_dbml(id, None)
                        .await
                        .map_err(|e| (-32000, e.to_string()))?;
                    return Ok(json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": "text/plain",
                            "text": dbml
                        }]
                    }));
                }
            }
            Err((-32602, format!("unknown resource `{uri}`")))
        }
        "prompts/list" => Ok(json!({ "prompts": [] })),
        other => Err((-32601, format!("method not found: {other}"))),
    }
}

pub(super) fn json_rpc_error(id: Option<JsonValue>, code: i64, message: &str) -> JsonValue {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(JsonValue::Null),
        "error": { "code": code, "message": message }
    })
}
