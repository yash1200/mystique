//! The ACP messages Mystique sends and reads, built from the pinned schema
//! crate (`agent-client-protocol-schema` 1.16.0, protocol version 1, no
//! unstable features). See `docs/ACP.md`.

use std::path::{Path, PathBuf};

use agent_client_protocol_schema::v1::{
    CancelNotification, ClientCapabilities, ContentBlock, EnvVariable, FileSystemCapabilities,
    HttpHeader, Implementation, InitializeRequest, InitializeResponse, McpServer, McpServerHttp,
    McpServerStdio, NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse,
    StopReason, SystemPrompt, TextContent,
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
    pub mcp_sse: bool,
    pub load_session: bool,
    pub auth_methods: Vec<String>,
    /// Takes a client system prompt on `session/new`
    /// (`sessionCapabilities.systemPrompt`).
    pub system_prompt: bool,
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
        mcp_sse: response.agent_capabilities.mcp_capabilities.sse,
        load_session: response.agent_capabilities.load_session,
        system_prompt: response
            .agent_capabilities
            .session_capabilities
            .system_prompt
            .is_some(),
        auth_methods: response
            .auth_methods
            .iter()
            .map(|m| m.name().to_string())
            .collect(),
    })
}

/// How the agent reaches Mystique's MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum McpRoute {
    /// Directly: `{ type: http, url, headers: [Authorization] }`.
    Http,
    /// Through the stdio bridge: the agent starts `program --mcp-stdio-bridge`
    /// with the URL and token in its environment.
    Bridge(PathBuf),
}

impl McpRoute {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Bridge(_) => "stdio",
        }
    }
}

/// HTTP when the agent takes it, else the stdio bridge when the app can run
/// one (stdio is the transport every ACP agent must support), else refused.
pub(crate) fn mcp_route(hello: &AgentHello, bridge: Option<&Path>) -> Result<McpRoute, String> {
    if hello.protocol_version != PROTOCOL.as_u16() {
        return Err(format!(
            "The agent speaks ACP version {}; Mystique speaks version {}.",
            hello.protocol_version,
            PROTOCOL.as_u16()
        ));
    }
    if hello.mcp_http {
        return Ok(McpRoute::Http);
    }
    match bridge {
        Some(program) => Ok(McpRoute::Bridge(program.to_path_buf())),
        None => Err(
            "The agent cannot connect to MCP servers over HTTP, and the stdio bridge is not available, so it cannot reach Mystique's database tools.".into(),
        ),
    }
}

/// `session/new`: the working directory and Mystique's MCP server, and no
/// other. Over HTTP the token is a header; over the bridge it is an
/// environment variable of the bridge process — never an argument.
///
/// `system`: Mystique's task contract, added to the agent's own
/// instructions (`append`). Pass it only when the agent advertises
/// `sessionCapabilities.systemPrompt`; ACP forbids it otherwise.
pub(crate) fn new_session_params(
    cwd: &Path,
    route: &McpRoute,
    mcp_url: &str,
    mcp_token: &str,
    system: Option<&str>,
) -> JsonValue {
    let server = match route {
        McpRoute::Http => {
            McpServer::Http(McpServerHttp::new(MCP_SERVER_NAME, mcp_url).headers(vec![
                HttpHeader::new("Authorization", format!("Bearer {mcp_token}")),
            ]))
        }
        McpRoute::Bridge(program) => McpServer::Stdio(
            McpServerStdio::new(MCP_SERVER_NAME, program.clone())
                .args(vec![crate::bridge::BRIDGE_FLAG.to_string()])
                .env(vec![
                    EnvVariable::new(crate::bridge::ENV_URL, mcp_url),
                    EnvVariable::new(crate::bridge::ENV_TOKEN, mcp_token),
                ]),
        ),
    };
    let system = system
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| SystemPrompt::new(vec![ContentBlock::Text(TextContent::new(text))]));
    let request = NewSessionRequest::new(cwd)
        .mcp_servers(vec![server])
        .system_prompt(system);
    serde_json::to_value(request).unwrap_or(JsonValue::Null)
}

/// The Test button's `session/new`: no MCP server at all. It only shows
/// whether the agent is signed in.
pub(crate) fn probe_session_params(cwd: &Path) -> JsonValue {
    serde_json::to_value(NewSessionRequest::new(cwd)).unwrap_or(JsonValue::Null)
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
        let params = new_session_params(
            Path::new("/tmp/acp"),
            &McpRoute::Http,
            "http://127.0.0.1:1/mcp",
            "tok",
            None,
        );
        assert_eq!(params["cwd"], "/tmp/acp");
        assert!(params.get("systemPrompt").is_none());
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
    fn over_the_bridge_the_token_is_in_the_environment_not_the_arguments() {
        let token = "tok-SECRET-123456";
        let exe = "/Applications/Mystique.app/Contents/MacOS/mystique";
        let params = new_session_params(
            Path::new("/tmp/acp"),
            &McpRoute::Bridge(PathBuf::from(exe)),
            "http://127.0.0.1:7801/mcp",
            token,
            None,
        );
        let servers = params["mcpServers"].as_array().unwrap();
        assert_eq!(servers.len(), 1);
        let server = &servers[0];
        assert_eq!(server["name"], MCP_SERVER_NAME);
        assert_eq!(server["command"], exe);
        assert_eq!(server["args"], json!([crate::bridge::BRIDGE_FLAG]));
        assert!(!server["args"].to_string().contains(token));
        assert_eq!(
            server["env"],
            json!([
                { "name": crate::bridge::ENV_URL, "value": "http://127.0.0.1:7801/mcp" },
                { "name": crate::bridge::ENV_TOKEN, "value": token }
            ])
        );
        assert!(server.get("headers").is_none());
        // The Test button's session has no MCP server.
        let probe = probe_session_params(Path::new("/tmp/acp"));
        assert_eq!(probe["mcpServers"], json!([]));
    }

    #[test]
    fn the_contract_goes_in_as_an_appended_system_prompt() {
        let params = new_session_params(
            Path::new("/tmp/acp"),
            &McpRoute::Http,
            "http://127.0.0.1:1/mcp",
            "tok",
            Some("Follow the task contract."),
        );
        assert_eq!(
            params["systemPrompt"],
            json!({
                "mode": "append",
                "content": [{ "type": "text", "text": "Follow the task contract." }]
            })
        );
        // Blank text sends no system prompt (ACP forbids an empty one).
        let blank = new_session_params(
            Path::new("/tmp/acp"),
            &McpRoute::Http,
            "http://127.0.0.1:1/mcp",
            "tok",
            Some("  "),
        );
        assert!(blank.get("systemPrompt").is_none());
    }

    #[test]
    fn the_system_prompt_capability_is_read_from_initialize() {
        let with = parse_initialize(json!({
            "protocolVersion": 1,
            "agentCapabilities": { "sessionCapabilities": { "systemPrompt": {} } },
            "agentInfo": { "name": "claude", "version": "1.0" }
        }))
        .unwrap();
        assert!(with.system_prompt);
        let without = parse_initialize(json!({
            "protocolVersion": 1,
            "agentCapabilities": {},
            "agentInfo": { "name": "grok", "version": "1.0" }
        }))
        .unwrap();
        assert!(!without.system_prompt);
    }

    #[test]
    fn http_wins_then_the_bridge_then_a_refusal() {
        let hello = parse_initialize(json!({
            "protocolVersion": 1,
            "agentCapabilities": { "mcpCapabilities": { "http": true, "sse": true } },
            "agentInfo": { "name": "fake", "version": "1.0" },
            "authMethods": [{ "id": "login", "name": "Log in" }]
        }))
        .unwrap();
        assert_eq!(hello.name, "fake");
        assert!(hello.mcp_sse);
        assert_eq!(hello.auth_methods, vec!["Log in".to_string()]);
        let bridge = Path::new("/app/mystique");
        assert_eq!(mcp_route(&hello, Some(bridge)), Ok(McpRoute::Http));
        assert_eq!(mcp_route(&hello, None), Ok(McpRoute::Http));
        let stdio_only = AgentHello {
            mcp_http: false,
            mcp_sse: false,
            ..hello.clone()
        };
        assert_eq!(
            mcp_route(&stdio_only, Some(bridge)),
            Ok(McpRoute::Bridge(bridge.to_path_buf()))
        );
        assert!(mcp_route(&stdio_only, None).unwrap_err().contains("HTTP"));
        let v2 = AgentHello {
            protocol_version: 2,
            ..hello
        };
        assert!(mcp_route(&v2, Some(bridge))
            .unwrap_err()
            .contains("version 2"));
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
