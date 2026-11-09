//! Agent Client Protocol (ACP) client: the agent pane can run an external
//! agent (Claude Code, Codex, Gemini CLI, Antigravity, Grok Build, …; see
//! `catalog.rs`) instead of the built-in one. Known agents are detected on
//! the machine and added with one click; agents without HTTP MCP reach
//! Mystique's tools through the stdio [`bridge`].
//!
//! Mystique is the ACP *client*. It starts the agent over stdio, declines
//! file system and terminal access, and passes Mystique's own MCP server
//! (with the MCP token) in `session/new`, so the only tools Mystique offers
//! are the database tools — behind the same access levels and approval dock
//! as any MCP client. Updates stream into the pane as the built-in agent's
//! [`mystique_agent::AgentEvent`]s. See `docs/ACP.md`.

#![deny(clippy::indexing_slicing, clippy::unwrap_used)]

pub mod bridge;
mod catalog;
mod detect;
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

pub use detect::{AcpAgentStatus, DetectedAcpAgent};
pub use process::Children;
pub use service::{AcpProbe, AcpService};
pub use session::McpAccess;
pub use settings::{
    looks_secret, AcpAgentSaved, AcpAgentView, AcpEnvInput, AcpEnvVar, AcpSettingsView,
    SaveAcpAgent, ACP_PROVIDER, BUILTIN,
};

/// Which catalog agents are on this machine, without an app (the
/// `detect_agents` example prints it). Blocking.
pub fn detect_agents() -> Vec<DetectedAcpAgent> {
    detect::detect()
}
