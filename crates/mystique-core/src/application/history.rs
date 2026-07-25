use crate::domain::{AppendHistoryInput, QueryHistoryItem};
use crate::error::AppResult;
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

pub struct HistoryService<'a> {
    pub(crate) db: &'a AppDatabase,
}

impl HistoryService<'_> {
    pub fn append(&self, input: AppendHistoryInput) -> AppResult<QueryHistoryItem> {
        self.db.with_conn(|c| repo::history::append(c, input))
    }

    pub fn list_for_connection(
        &self,
        connection_id: &str,
        limit: i64,
    ) -> AppResult<Vec<QueryHistoryItem>> {
        self.db
            .with_conn(|c| repo::history::list_for_connection(c, connection_id, limit))
    }

    pub fn list_recent(&self, limit: i64) -> AppResult<Vec<QueryHistoryItem>> {
        self.db.with_conn(|c| repo::history::list_recent(c, limit))
    }

    pub fn clear_for_connection(&self, connection_id: &str) -> AppResult<usize> {
        self.db
            .with_conn(|c| repo::history::clear_for_connection(c, connection_id))
    }

    pub fn retain_latest(&self, keep: i64) -> AppResult<usize> {
        self.db.with_conn(|c| repo::history::retain_latest(c, keep))
    }
}
