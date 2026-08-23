//! Application services: use-cases on top of storage + connectors.
//!
//! IPC / HTTP adapters call these — never repositories or drivers directly.

mod agent_chat;
mod agent_tools;
mod ai_secrets;
mod dbml;
mod history;
mod profiles;
mod query_library;
mod settings;
mod workspace_tabs;

pub use agent_chat::AgentChatService;
pub use ai_secrets::AiSecretsService;
pub use agent_tools::{
    tool_specs, AgentColumnInfo, AgentConnectionInfo, AgentObjectInfo, AgentQueryDownload,
    AgentToolSpec, AgentTools,
};
pub use history::HistoryService;
pub use profiles::ProfileService;
pub use query_library::QueryLibraryService;
pub use settings::SettingsService;
pub use workspace_tabs::WorkspaceTabsService;

use std::sync::Arc;

use crate::connectors::ConnectionManager;
use crate::storage::AppDatabase;

/// Bundle of application services.
///
/// Held in Tauri state (or tests) as `Arc<AppServices>`.
pub struct AppServices {
    pub db: Arc<AppDatabase>,
    pub connections: Arc<ConnectionManager>,
}

impl AppServices {
    pub fn new(db: AppDatabase) -> Self {
        Self {
            db: Arc::new(db),
            connections: Arc::new(ConnectionManager::bootstrap()),
        }
    }

    pub fn with_connections(db: AppDatabase, connections: ConnectionManager) -> Self {
        Self {
            db: Arc::new(db),
            connections: Arc::new(connections),
        }
    }

    pub fn agent_tools(&self) -> AgentTools {
        AgentTools::new(Arc::clone(&self.db), Arc::clone(&self.connections))
    }

    pub fn profiles(&self) -> ProfileService<'_> {
        ProfileService { db: &self.db }
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
