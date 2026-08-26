use std::sync::Arc;

use mystique_core::{AgentTools, AppServices};

use crate::runtime::AgentRuntime;
use crate::secrets::SecretStore;

pub fn runtime_enabled(app: &AppServices) -> bool {
    AgentRuntime::enabled(app)
}

/// Host-facing agent service (CRUD + send).
#[derive(Clone)]
pub struct AgentService {
    app: Arc<AppServices>,
    secrets: SecretStore,
    runtime: AgentRuntime,
}

impl AgentService {
    pub fn new(app: Arc<AppServices>) -> Self {
        Self::with_secrets(app.clone(), SecretStore::new(app))
    }

    /// Import leftover plaintext keys from the old `ai_secrets.json` file once.
    pub fn with_legacy_file(app: Arc<AppServices>, path: impl Into<std::path::PathBuf>) -> Self {
        Self::with_secrets(app.clone(), SecretStore::with_legacy_file(app, path))
    }

    fn with_secrets(app: Arc<AppServices>, secrets: SecretStore) -> Self {
        Self {
            app,
            secrets,
            runtime: AgentRuntime::new(),
        }
    }

    pub fn tools(&self) -> AgentTools {
        self.app.agent_tools()
    }

    pub fn secrets(&self) -> &SecretStore {
        &self.secrets
    }

    pub fn runtime(&self) -> &AgentRuntime {
        &self.runtime
    }

    pub fn app(&self) -> &AppServices {
        &self.app
    }
}
