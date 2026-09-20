use mystique_core::{parse_allowlist, AgentWritePolicy};
use serde::{Deserialize, Serialize};

pub const DEFAULT_MCP_PORT: u16 = 17321;

pub const SETTING_MCP_ENABLED: &str = "mcp.enabled";
pub const SETTING_MCP_PORT: &str = "mcp.port";
pub const SETTING_MCP_TOKEN: &str = "mcp.token";

pub use mystique_core::{SETTING_MCP_ALLOWLIST, SETTING_MCP_POLICY};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAllowlistConnection {
    pub id: String,
    pub name: String,
    pub engine: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAccess {
    pub policy: String,
    pub allowlist: Vec<String>,
    pub connections: Vec<McpAllowlistConnection>,
}

pub fn canonical_policy(raw: &str) -> AgentWritePolicy {
    AgentWritePolicy::parse(raw)
}

pub fn canonical_allowlist(ids: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = ids
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}

pub fn allowlist_from_setting(raw: &str) -> Vec<String> {
    canonical_allowlist(parse_allowlist(raw))
}
