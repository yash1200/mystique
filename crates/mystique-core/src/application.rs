//! Application services: use-cases on top of storage + connectors.
//!
//! IPC / HTTP adapters call these — never repositories or drivers directly.

mod agent_chat;
mod agent_tools;
mod ai_secrets;
pub(crate) mod dbml;
mod history;
mod profiles;
mod query_library;
mod settings;
mod workspace_tabs;

pub use agent_chat::AgentChatService;
pub use agent_tools::{
    tool_specs, AgentColumnInfo, AgentConnectionInfo, AgentObjectInfo, AgentQueryDownload,
    AgentToolSpec, AgentTools,
};
pub use ai_secrets::AiSecretsService;
pub use history::HistoryService;
pub use profiles::ProfileService;
pub use query_library::QueryLibraryService;
pub use settings::SettingsService;
pub use workspace_tabs::WorkspaceTabsService;

use std::sync::Arc;

use crate::connectors::{ConnectionManager, OpenSessionRequest};
use crate::error::AppResult;
use crate::secrets::SecretStore;
use crate::storage::AppDatabase;

/// Bundle of application services.
///
/// Held in Tauri state (or tests) as `Arc<AppServices>`.
pub struct AppServices {
    pub db: Arc<AppDatabase>,
    pub connections: Arc<ConnectionManager>,
    pub secrets: Arc<SecretStore>,
}

impl AppServices {
    pub fn new(db: AppDatabase) -> Self {
        Self::with_secrets(db, ConnectionManager::bootstrap(), SecretStore::os())
    }

    pub fn with_connections(db: AppDatabase, connections: ConnectionManager) -> Self {
        Self::with_secrets(db, connections, SecretStore::os())
    }

    pub fn with_secrets(
        db: AppDatabase,
        connections: ConnectionManager,
        secrets: SecretStore,
    ) -> Self {
        let svc = Self {
            db: Arc::new(db),
            connections: Arc::new(connections),
            secrets: Arc::new(secrets),
        };
        let _ = svc.profiles().migrate_plaintext_secrets();
        svc
    }

    pub fn agent_tools(&self) -> AgentTools {
        AgentTools::new(
            Arc::clone(&self.db),
            Arc::clone(&self.connections),
            Arc::clone(&self.secrets),
        )
    }

    pub fn profiles(&self) -> ProfileService<'_> {
        ProfileService {
            db: &self.db,
            secrets: &self.secrets,
        }
    }

    /// Typed request password wins; otherwise look up the profile in the keychain.
    pub fn password_for_request(&self, req: &OpenSessionRequest) -> AppResult<Option<String>> {
        let typed = req
            .password
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(pw) = typed {
            return Ok(Some(pw.to_string()));
        }
        let id = req
            .profile_id
            .as_deref()
            .or(req.session_id.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        match id {
            Some(id) => self.profiles().resolve_password(id),
            None => Ok(None),
        }
    }

    pub fn query_library(&self) -> QueryLibraryService<'_> {
        QueryLibraryService { db: &self.db }
    }

    pub fn workspace_tabs(&self) -> WorkspaceTabsService<'_> {
        WorkspaceTabsService { db: &self.db }
    }

    pub fn history(&self) -> HistoryService<'_> {
        HistoryService { db: &self.db }
    }

    pub fn settings(&self) -> SettingsService<'_> {
        SettingsService { db: &self.db }
    }

    pub fn agent_chats(&self) -> AgentChatService<'_> {
        AgentChatService { db: &self.db }
    }

    pub fn ai_secrets(&self) -> AiSecretsService<'_> {
        AiSecretsService { db: &self.db }
    }

    pub fn connections(&self) -> Arc<ConnectionManager> {
        Arc::clone(&self.connections)
    }
}
