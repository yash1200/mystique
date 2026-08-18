use crate::domain::{ReplaceWorkspaceTabsInput, WorkspaceTabRecord};
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

    pub fn delete_tab(&self, id: &str) -> AppResult<()> {
        self.db
            .with_conn(|c| repo::workspace_tabs::delete_tab(c, id))
    }
}
