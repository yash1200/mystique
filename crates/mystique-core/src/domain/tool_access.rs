//! Access levels and approval requests for MCP and the built-in agent (F-02).

use serde::{Deserialize, Serialize};

/// The settings pane: both global defaults and every connection's levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ToolAccessView {
    /// `none` | `read` | `write` | `full`.
    pub mcp_default: String,
    pub agent_default: String,
    pub connections: Vec<ToolAccessConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ToolAccessConnection {
    pub id: String,
    pub name: String,
    pub engine: String,
    /// `local` | `staging` | `production`.
    pub environment: String,
    /// The profile's default database, when it names one.
    pub default_database: Option<String>,
    /// Set on the connection. Absent = inherits the global default.
    pub level: Option<String>,
    /// What MCP gets on the connection (after inheritance).
    pub mcp_level: String,
    /// What the built-in agent gets on the connection (after inheritance).
    pub agent_level: String,
    /// Writes above the level go to the approval dock instead of failing.
    pub ask_high_risk: bool,
    /// Levels set on single databases.
    pub databases: Vec<ToolAccessDatabase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ToolAccessDatabase {
    pub name: String,
    pub level: String,
}

/// One write waiting in the approval dock. Never persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalRequest {
    pub id: String,
    /// `mcp` | `agent`.
    pub caller: String,
    pub connection_id: String,
    pub connection_name: String,
    pub database: Option<String>,
    pub environment: String,
    /// Exactly the text that runs if approved.
    pub sql: String,
    /// The level the statement needs, and the level the caller has.
    pub needed: String,
    pub granted: String,
    pub requested_at: String,
    /// After this the request is denied.
    pub expires_at: String,
}
