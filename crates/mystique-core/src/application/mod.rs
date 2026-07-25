//! Application services: use-cases on top of storage.
//!
//! IPC / HTTP / CLI adapters call these — never repositories directly.
//! Later: inject keychain, engine registry, audit sinks here.

mod history;
mod profiles;
mod query_library;
mod settings;

pub use history::HistoryService;
pub use profiles::ProfileService;
pub use query_library::QueryLibraryService;
pub use settings::SettingsService;

use std::sync::Arc;

use crate::storage::AppDatabase;

/// Bundle of application services sharing one database.
///
/// Held in Tauri state (or tests) as `Arc<AppServices>`.
pub struct AppServices {
    pub db: Arc<AppDatabase>,
}

impl AppServices {
    pub fn new(db: AppDatabase) -> Self {
        Self { db: Arc::new(db) }
    }

    pub fn profiles(&self) -> ProfileService<'_> {
        ProfileService { db: &self.db }
    }

    pub fn query_library(&self) -> QueryLibraryService<'_> {
        QueryLibraryService { db: &self.db }
    }

    pub fn history(&self) -> HistoryService<'_> {
        HistoryService { db: &self.db }
    }

    pub fn settings(&self) -> SettingsService<'_> {
        SettingsService { db: &self.db }
    }
}
