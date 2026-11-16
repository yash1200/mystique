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

pub mod kit;

pub use providers::{
    default_model, models_for, parse_provider, provider_catalog, AgentModelInfo, AgentProvider,
    AgentProviderInfo,
};
pub use runtime::{friendly_error, provider_error_summary};
pub use runtime::{
    AgentErrorKind, AgentEvent, AgentEventSink, AgentFixRequest, AgentRuntime, AgentTableRef,
    AgentTokens, SendAgentMessage, SendAgentResult, TurnStart, DEFAULT_OLLAMA_URL,
    SETTING_AI_ENABLED, SETTING_OLLAMA_URL,
};
pub use secrets::{AiKeyStatus, AiSettingsView, SecretStore};
pub use service::{runtime_enabled, AgentService};

/// Internals for `tests/` (feature `test-support`, which only this crate's
/// dev-dependency on itself turns on): the model adapter, the streaming
/// loop with its seams, and the bound tools.
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support {
    pub use crate::runtime::host::{RunHost, ToolFailure, ToolRunner};
    pub use crate::runtime::model::{
        boxed, BoxFuture, Delta, DeltaSink, TurnError, TurnModel, TurnOutput, TurnRequest,
    };
    pub use crate::runtime::prompt::ErrorAt;
    pub use crate::runtime::runs::{execute_run, RunRegistry, RunSpec};
    pub use crate::runtime::turns::LoopConfig;
    pub use crate::tools::{BoundTools, ToolErr, WRITE_TOOL};
}
