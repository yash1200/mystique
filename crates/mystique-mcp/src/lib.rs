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
pub use settings::DEFAULT_MCP_PORT;

/// The JSON-RPC handler, for `tests/server.rs` (feature `test-support`,
/// which only this crate's dev-dependency on itself turns on).
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support {
    pub use crate::server::rpc::handle_method;
}
