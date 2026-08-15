//! Localhost MCP HTTP server for external LLM clients.
//!
//! In-app agents must call [`mystique_core::AgentTools`] directly — not this crate.

mod server;
mod service;

pub use server::{McpController, McpStatus, DEFAULT_MCP_PORT};
pub use service::McpService;
