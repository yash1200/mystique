//! Localhost MCP HTTP server for external LLM clients.
//!
//! In-app agents must call [`mystique_core::AgentTools`] directly — not this crate.

mod server;
mod service;
mod settings;

pub use server::McpStatus;
pub use service::McpService;
pub use settings::DEFAULT_MCP_PORT;
