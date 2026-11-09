//! Agent Client Protocol (ACP) client: the agent pane can run an external
//! agent (Claude Code through its ACP adapter) instead of the built-in one.
//!
//! Mystique is the ACP *client*. It starts the agent over stdio, declines
//! file system and terminal access, and passes Mystique's own MCP server
//! (with the MCP token) in `session/new`, so the only tools Mystique offers
//! are the database tools — behind the same access levels and approval dock
//! as any MCP client. Updates stream into the pane as the built-in agent's
//! [`mystique_agent::AgentEvent`]s. See `docs/ACP.md`.

#![deny(clippy::indexing_slicing, clippy::unwrap_used)]

mod mapping;
mod permission;
mod process;
mod protocol;
mod rpc;
mod scrub;
mod service;
mod session;
mod settings;

#[cfg(test)]
mod tests;

pub use process::Children;
pub use service::{AcpProbe, AcpService};
pub use session::McpAccess;
pub use settings::{
    looks_secret, AcpEnvInput, AcpEnvVar, AcpSettingsView, SaveAcpSettings, ACP_PROVIDER, KIND_ACP,
    KIND_BUILTIN, SETTING_AGENT_KIND,
};
