//! The ACP messages Mystique sends and reads, built from the pinned schema
//! crate (`agent-client-protocol-schema` 1.16.0, protocol version 1, no
//! unstable features). See `docs/ACP.md`.

use std::path::Path;

use agent_client_protocol_schema::v1::{
    CancelNotification, ClientCapabilities, ContentBlock, FileSystemCapabilities, HttpHeader,
    Implementation, InitializeRequest, InitializeResponse, McpServer, McpServerHttp,
    NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, StopReason, TextContent,
};
use agent_client_protocol_schema::ProtocolVersion;
use serde_json::Value as JsonValue;

/// The one protocol revision Mystique speaks.
pub(crate) const PROTOCOL: ProtocolVersion = ProtocolVersion::V1;

/// The name Mystique's MCP server has inside the agent's session. Matches
/// the name Settings → MCP installs into harnesses.
pub(crate) const MCP_SERVER_NAME: &str = "mystique";

pub(crate) const INITIALIZE: &str = "initialize";
pub(crate) const SESSION_NEW: &str = "session/new";
pub(crate) const SESSION_PROMPT: &str = "session/prompt";
pub(crate) const SESSION_CANCEL: &str = "session/cancel";
pub(crate) const SESSION_UPDATE: &str = "session/update";
pub(crate) const REQUEST_PERMISSION: &str = "session/request_permission";

/// ACP's "authentication required" error code.
pub(crate) const AUTH_REQUIRED: i64 = -32000;

/// `initialize` params. File system and terminal are declined: the agent
/// cannot read or write files or run commands through Mystique.
pub(crate) fn initialize_params() -> JsonValue {
    let capabilities = ClientCapabilities::new()
        .fs(FileSystemCapabilities::new()
            .read_text_file(false)
            .write_text_file(false))
        .terminal(false);
    let request = InitializeRequest::new(PROTOCOL)
        .client_capabilities(capabilities)
        .client_info(
            Implementation::new("mystique", env!("CARGO_PKG_VERSION"))
                .title("Mystique".to_string()),
        );
    serde_json::to_value(request).unwrap_or(JsonValue::Null)
}

/// What the agent said about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentHello {
    pub name: String,
    pub title: Option<String>,
    pub version: String,
    pub protocol_version: u16,
    pub mcp_http: bool,
    pub load_session: bool,
    pub auth_methods: Vec<String>,
}

pub(crate) fn parse_initialize(result: JsonValue) -> Result<AgentHello, String> {
    let response: InitializeResponse = serde_json::from_value(result)
        .map_err(|e| format!("The agent's initialize reply is not ACP: {e}"))?;
    let info = response.agent_info.as_ref();
    Ok(AgentHello {
        name: info.map(|i| i.name.clone()).unwrap_or_default(),
        title: info.and_then(|i| i.title.clone()),
        version: info.map(|i| i.version.clone()).unwrap_or_default(),
        protocol_version: response.protocol_version.as_u16(),
        mcp_http: response.agent_capabilities.mcp_capabilities.http,
        load_session: response.agent_capabilities.load_session,
        auth_methods: response
            .auth_methods
            .iter()
            .map(|m| m.name().to_string())
            .collect(),
    })
}

/// Why this agent cannot run a Mystique turn, if it cannot.
pub(crate) fn unsupported(hello: &AgentHello) -> Option<String> {
    if hello.protocol_version != PROTOCOL.as_u16() {
        return Some(format!(
            "The agent speaks ACP version {}; Mystique speaks version {}.",
            hello.protocol_version,
            PROTOCOL.as_u16()
        ));
    }
    if !hello.mcp_http {
        return Some(
            "The agent cannot connect to MCP servers over HTTP, so it cannot reach Mystique's database tools.".into(),
        );
    }
    None
}

/// `session/new`: the working directory and Mystique's MCP server (HTTP,
/// bearer token). No other MCP server is passed.
pub(crate) fn new_session_params(cwd: &Path, mcp_url: &str, mcp_token: &str) -> JsonValue {
    let server = McpServerHttp::new(MCP_SERVER_NAME, mcp_url).headers(vec![HttpHeader::new(
        "Authorization",
        format!("Bearer {mcp_token}"),
    )]);
    let request = NewSessionRequest::new(cwd).mcp_servers(vec![McpServer::Http(server)]);
    serde_json::to_value(request).unwrap_or(JsonValue::Null)
}

pub(crate) fn parse_new_session(result: JsonValue) -> Result<String, String> {
    let response: NewSessionResponse = serde_json::from_value(result)
        .map_err(|e| format!("The agent's session/new reply is not ACP: {e}"))?;
    Ok(response.session_id.0.to_string())
}

/// `session/prompt` with text blocks only (every agent must accept text).
pub(crate) fn prompt_params(session_id: &str, blocks: &[String]) -> JsonValue {
    let prompt = blocks
        .iter()
        .filter(|b| !b.trim().is_empty())
        .map(|b| ContentBlock::Text(TextContent::new(b.clone())))
        .collect();
    serde_json::to_value(PromptRequest::new(session_id.to_string(), prompt))
        .unwrap_or(JsonValue::Null)
}

pub(crate) fn cancel_params(session_id: &str) -> JsonValue {
    serde_json::to_value(CancelNotification::new(session_id.to_string())).unwrap_or(JsonValue::Null)
}

/// How the prompt turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TurnEnd {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
}

pub(crate) fn parse_prompt_response(result: JsonValue) -> Result<TurnEnd, String> {
    let response: PromptResponse = serde_json::from_value(result)
        .map_err(|e| format!("The agent's session/prompt reply is not ACP: {e}"))?;
    Ok(match response.stop_reason {
        StopReason::EndTurn => TurnEnd::EndTurn,
        StopReason::MaxTokens => TurnEnd::MaxTokens,
        StopReason::MaxTurnRequests => TurnEnd::MaxTurnRequests,
        StopReason::Refusal => TurnEnd::Refusal,
        StopReason::Cancelled => TurnEnd::Cancelled,
        _ => TurnEnd::EndTurn,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn initialize_declines_files_and_terminal() {
        let params = initialize_params();
        assert_eq!(params["protocolVersion"], json!(1));
        assert_eq!(
            params["clientCapabilities"]["fs"],
            json!({ "readTextFile": false, "writeTextFile": false })
        );
        assert_eq!(params["clientCapabilities"]["terminal"], json!(false));
        assert_eq!(params["clientInfo"]["name"], "mystique");
    }

    #[test]
    fn new_session_passes_only_mystique_over_http_with_the_token() {
        let params = new_session_params(Path::new("/tmp/acp"), "http://127.0.0.1:1/mcp", "tok");
        assert_eq!(params["cwd"], "/tmp/acp");
        let servers = params["mcpServers"].as_array().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0]["type"], "http");
        assert_eq!(servers[0]["name"], MCP_SERVER_NAME);
        assert_eq!(servers[0]["url"], "http://127.0.0.1:1/mcp");
        assert_eq!(
            servers[0]["headers"],
            json!([{ "name": "Authorization", "value": "Bearer tok" }])
        );
    }

    #[test]
    fn an_agent_without_http_mcp_or_on_another_version_is_refused() {
        let hello = parse_initialize(json!({
            "protocolVersion": 1,
            "agentCapabilities": { "mcpCapabilities": { "http": true } },
            "agentInfo": { "name": "fake", "version": "1.0" },
            "authMethods": [{ "id": "login", "name": "Log in" }]
        }))
        .unwrap();
        assert_eq!(hello.name, "fake");
        assert_eq!(hello.auth_methods, vec!["Log in".to_string()]);
        assert_eq!(unsupported(&hello), None);
        let no_http = AgentHello {
            mcp_http: false,
            ..hello.clone()
        };
        assert!(unsupported(&no_http).unwrap().contains("HTTP"));
        let v2 = AgentHello {
            protocol_version: 2,
            ..hello
        };
        assert!(unsupported(&v2).unwrap().contains("version 2"));
    }

    #[test]
    fn stop_reasons_parse() {
        for (raw, end) in [
            ("end_turn", TurnEnd::EndTurn),
            ("max_tokens", TurnEnd::MaxTokens),
            ("max_turn_requests", TurnEnd::MaxTurnRequests),
            ("refusal", TurnEnd::Refusal),
            ("cancelled", TurnEnd::Cancelled),
        ] {
            assert_eq!(
                parse_prompt_response(json!({ "stopReason": raw })).unwrap(),
                end
            );
        }
    }
}
