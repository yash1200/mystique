//! In-app query-tab agent.
//!
//! Calls [`mystique_core::AgentTools`] directly (never the MCP HTTP server).
//! LLM traffic goes through Rig.

mod providers;
mod runtime;
mod secrets;
mod tools;

pub use providers::{
    default_model, models_for, parse_provider, provider_catalog, AgentModelInfo, AgentProvider,
    AgentProviderInfo,
};
pub use runtime::{
    SendAgentMessage, SendAgentResult, DEFAULT_OLLAMA_URL, SETTING_AI_ENABLED, SETTING_OLLAMA_URL,
};
pub use secrets::{AiKeyStatus, SecretStore};

use std::sync::Arc;

use mystique_core::{AgentTools, AppServices};

use crate::runtime::AgentRuntime;
use crate::secrets::SecretStore as KeyStore;

pub fn runtime_enabled(app: &AppServices) -> bool {
    AgentRuntime::enabled(app)
}

/// Host-facing agent service (CRUD + send).
#[derive(Clone)]
pub struct AgentService {
    app: Arc<AppServices>,
    secrets: KeyStore,
    runtime: AgentRuntime,
}

impl AgentService {
    pub fn new(app: Arc<AppServices>) -> Self {
        Self::with_secrets(app.clone(), KeyStore::new(app))
    }

    /// Import leftover plaintext keys from the old `ai_secrets.json` file once.
    pub fn with_legacy_file(
        app: Arc<AppServices>,
        path: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self::with_secrets(app.clone(), KeyStore::with_legacy_file(app, path))
    }

    fn with_secrets(app: Arc<AppServices>, secrets: KeyStore) -> Self {
        Self {
            app,
            secrets,
            runtime: AgentRuntime::new(),
        }
    }

    pub fn tools(&self) -> AgentTools {
        self.app.agent_tools()
    }

    pub fn secrets(&self) -> &KeyStore {
        &self.secrets
    }

    pub fn runtime(&self) -> &AgentRuntime {
        &self.runtime
    }

    pub fn app(&self) -> &AppServices {
        &self.app
    }
}
