//! Which of the agent's tool calls are Mystique's, and what Mystique answers
//! when the agent asks permission for one.
//!
//! Policy: a call to one of Mystique's own MCP tools gets the agent's
//! "allow once" option. Mystique's access levels and approval dock still
//! decide what that call may do when it reaches the MCP server, so allowing
//! it here grants nothing the levels do not. Everything else (the agent's
//! file, shell and web tools, other MCP servers) is rejected. Mystique never
//! picks an "always" option.

use std::sync::OnceLock;

use agent_client_protocol_schema::v1::{PermissionOptionKind, RequestPermissionRequest};
use mystique_core::AgentTools;
use serde_json::{json, Value as JsonValue};

use crate::protocol::MCP_SERVER_NAME;

fn mystique_tools() -> &'static Vec<&'static str> {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| AgentTools::specs().iter().map(|s| s.name).collect())
}

/// The Mystique tool behind a server-qualified name, e.g.
/// `mcp__mystique__execute_query` (Claude Code) or `mystique/execute_query`.
/// A bare `execute_query` does not count: another server could use it.
fn qualified(candidate: &str) -> Option<&'static str> {
    let candidate = candidate.trim();
    let prefixes = [
        format!("mcp__{MCP_SERVER_NAME}__"),
        format!("{MCP_SERVER_NAME}__"),
        format!("{MCP_SERVER_NAME}/"),
        format!("{MCP_SERVER_NAME}."),
        format!("{MCP_SERVER_NAME}:"),
    ];
    let rest = prefixes
        .iter()
        .find_map(|p| candidate.strip_prefix(p.as_str()))?;
    mystique_tools().iter().copied().find(|name| *name == rest)
}

/// Every string field in `_meta` called `toolName` (Claude Code puts the
/// MCP-qualified name in `_meta.claudeCode.toolName`).
fn meta_tool_names(meta: Option<&serde_json::Map<String, JsonValue>>) -> Vec<String> {
    let mut out = Vec::new();
    let Some(meta) = meta else { return out };
    for value in meta.values() {
        if let Some(name) = value.get("toolName").and_then(JsonValue::as_str) {
            out.push(name.to_string());
        }
    }
    if let Some(name) = meta.get("toolName").and_then(JsonValue::as_str) {
        out.push(name.to_string());
    }
    out
}

/// What a tool call is called, from its name, `_meta` and title.
pub(crate) fn identify(
    name: Option<&str>,
    title: Option<&str>,
    meta: Option<&serde_json::Map<String, JsonValue>>,
) -> ToolIdentity {
    let mut candidates: Vec<String> = name.map(str::to_owned).into_iter().collect();
    candidates.extend(meta_tool_names(meta));
    candidates.extend(title.map(str::to_owned));
    if let Some(tool) = candidates.iter().find_map(|c| qualified(c)) {
        return ToolIdentity::Mystique(tool);
    }
    let label = title
        .or(name)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("tool")
        .chars()
        .take(120)
        .collect();
    ToolIdentity::Other(label)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolIdentity {
    /// One of Mystique's MCP tools, by its plain name.
    Mystique(&'static str),
    /// Anything else, with a label for the card.
    Other(String),
}

impl ToolIdentity {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Mystique(name) => (*name).to_string(),
            Self::Other(label) => label.clone(),
        }
    }

    pub(crate) fn is_mystique(&self) -> bool {
        matches!(self, Self::Mystique(_))
    }
}

/// Mystique's answer to `session/request_permission`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Answer {
    pub allowed: bool,
    pub tool: String,
    /// The `RequestPermissionResponse` body.
    pub result: JsonValue,
}

fn selected(option_id: &str) -> JsonValue {
    json!({ "outcome": { "outcome": "selected", "optionId": option_id } })
}

fn cancelled() -> JsonValue {
    json!({ "outcome": { "outcome": "cancelled" } })
}

/// Answer one permission request. `known` names the call from its earlier
/// `tool_call` notification, when there was one.
pub(crate) fn answer(params: &JsonValue, known: Option<&ToolIdentity>) -> Answer {
    let Ok(request) = serde_json::from_value::<RequestPermissionRequest>(params.clone()) else {
        return Answer {
            allowed: false,
            tool: "tool".into(),
            result: cancelled(),
        };
    };
    let call = &request.tool_call;
    let identity = match known {
        Some(id @ ToolIdentity::Mystique(_)) => id.clone(),
        _ => identify(
            call.fields.name.as_deref(),
            call.fields.title.as_deref(),
            call.meta.as_ref(),
        ),
    };
    let pick = |kind: PermissionOptionKind| {
        request
            .options
            .iter()
            .find(|o| o.kind == kind)
            .map(|o| o.option_id.0.to_string())
    };
    if identity.is_mystique() {
        if let Some(id) = pick(PermissionOptionKind::AllowOnce) {
            return Answer {
                allowed: true,
                tool: identity.label(),
                result: selected(&id),
            };
        }
    }
    let result = pick(PermissionOptionKind::RejectOnce)
        .or_else(|| pick(PermissionOptionKind::RejectAlways))
        .map_or_else(cancelled, |id| selected(&id));
    Answer {
        allowed: false,
        tool: identity.label(),
        result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(tool_call: JsonValue) -> JsonValue {
        json!({
            "sessionId": "s1",
            "toolCall": tool_call,
            "options": [
                { "optionId": "always", "name": "Always allow", "kind": "allow_always" },
                { "optionId": "once", "name": "Allow", "kind": "allow_once" },
                { "optionId": "no", "name": "Reject", "kind": "reject_once" }
            ]
        })
    }

    #[test]
    fn a_mystique_mcp_tool_is_allowed_once_never_always() {
        let a = answer(
            &request(json!({
                "toolCallId": "c1",
                "title": "Run query",
                "_meta": { "claudeCode": { "toolName": "mcp__mystique__execute_query" } }
            })),
            None,
        );
        assert!(a.allowed);
        assert_eq!(a.tool, "execute_query");
        assert_eq!(a.result["outcome"]["optionId"], "once");
    }

    #[test]
    fn files_shell_and_other_servers_are_rejected() {
        for call in [
            json!({ "toolCallId": "c", "title": "Bash", "kind": "execute",
                    "_meta": { "claudeCode": { "toolName": "Bash" } } }),
            json!({ "toolCallId": "c", "title": "Write /etc/hosts", "kind": "edit" }),
            json!({ "toolCallId": "c", "title": "mcp__other__execute_query" }),
            // A bare name could belong to any server.
            json!({ "toolCallId": "c", "title": "execute_query" }),
            json!({ "toolCallId": "c", "title": "mcp__mystique__drop_everything" }),
        ] {
            let a = answer(&request(call.clone()), None);
            assert!(!a.allowed, "{call}");
            assert_eq!(a.result["outcome"]["optionId"], "no", "{call}");
        }
    }

    #[test]
    fn without_a_reject_option_the_request_is_cancelled() {
        let a = answer(
            &json!({
                "sessionId": "s1",
                "toolCall": { "toolCallId": "c", "title": "Bash" },
                "options": [{ "optionId": "ok", "name": "Allow", "kind": "allow_once" }]
            }),
            None,
        );
        assert!(!a.allowed);
        assert_eq!(a.result, json!({ "outcome": { "outcome": "cancelled" } }));
        let junk = answer(&json!({ "nope": true }), None);
        assert!(!junk.allowed);
    }

    #[test]
    fn an_earlier_tool_call_names_a_terse_request() {
        let known = ToolIdentity::Mystique("sample_rows");
        let a = answer(&request(json!({ "toolCallId": "c1" })), Some(&known));
        assert!(a.allowed);
        assert_eq!(a.tool, "sample_rows");
    }
}
