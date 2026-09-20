//! Localhost MCP HTTP server for external LLM clients.
//!
//! In-app agents must call [`mystique_core::AgentTools`] directly — not this crate.

mod clients;
mod server;
mod service;
mod settings;

pub use clients::{McpEndpoint, McpHarness};
pub use server::McpStatus;
pub use service::McpService;
pub use settings::{McpAccess, McpAllowlistConnection, DEFAULT_MCP_PORT};
