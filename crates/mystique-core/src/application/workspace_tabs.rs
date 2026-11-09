use crate::domain::{ReplaceWorkspaceTabsInput, SaveWorkspaceTabInput, WorkspaceTabRecord};
use crate::error::AppResult;
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

pub struct WorkspaceTabsService<'a> {
    pub(crate) db: &'a AppDatabase,
}

impl WorkspaceTabsService<'_> {
    pub fn list_tabs(&self, connection_id: &str) -> AppResult<Vec<WorkspaceTabRecord>> {
        self.db
            .with_conn(|c| repo::workspace_tabs::list_tabs(c, connection_id))
    }

    pub fn replace_tabs(
        &self,
        input: ReplaceWorkspaceTabsInput,
    ) -> AppResult<Vec<WorkspaceTabRecord>> {
        self.db
            .with_conn(|c| repo::workspace_tabs::replace_tabs(c, input))
    }

    pub fn upsert_tab(&self, input: SaveWorkspaceTabInput) -> AppResult<WorkspaceTabRecord> {
        self.db
            .with_conn(|c| repo::workspace_tabs::upsert_one(c, input))
    }
}
