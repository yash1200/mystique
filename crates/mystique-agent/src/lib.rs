//! In-app query-tab agent.
//!
//! Calls [`mystique_core::AgentTools`] directly (never the MCP HTTP server).
//! LLM traffic goes through Rig.

#![deny(clippy::indexing_slicing, clippy::unwrap_used)]

mod providers;
mod runtime;
mod secrets;
mod service;
mod tools;

pub use providers::{
    default_model, models_for, parse_provider, provider_catalog, AgentModelInfo, AgentProvider,
    AgentProviderInfo,
};
pub use runtime::{
    AgentRuntime, SendAgentMessage, SendAgentResult, DEFAULT_OLLAMA_URL, SETTING_AI_ENABLED,
    SETTING_OLLAMA_URL,
};
pub use secrets::{AiKeyStatus, SecretStore};
pub use service::{runtime_enabled, AgentService};
