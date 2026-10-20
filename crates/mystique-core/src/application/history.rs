use std::sync::Arc;

use crate::connectors::{ConnectionManager, QueryHistoryWriter};
use crate::domain::{AppendHistoryInput, HistoryList, QueryHistoryItem, QueryHistoryPage};
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

    pub fn list_page(&self, query: HistoryList) -> AppResult<QueryHistoryPage> {
        self.db.with_conn(|c| repo::history::list_page(c, &query))
    }

    pub fn list_for_connection(
        &self,
        connection_id: &str,
        limit: i64,
    ) -> AppResult<Vec<QueryHistoryItem>> {
        Ok(self
            .list_page(HistoryList {
                connection_id: Some(connection_id.to_string()),
                limit,
                search: None,
                cursor_ran_at: None,
                cursor_id: None,
            })?
            .items)
    }

    pub fn list_recent(&self, limit: i64) -> AppResult<Vec<QueryHistoryItem>> {
        Ok(self
            .list_page(HistoryList {
                connection_id: None,
                limit,
                search: None,
                cursor_ran_at: None,
                cursor_id: None,
            })?
            .items)
    }

    pub fn clear_for_connection(&self, connection_id: &str) -> AppResult<usize> {
        self.db
            .with_conn(|c| repo::history::clear_for_connection(c, connection_id))
    }

    /// Keep the latest `keep` rows on each connection.
    pub fn retain_latest(&self, keep: i64) -> AppResult<usize> {
        self.db.with_conn(|c| repo::history::retain_latest(c, keep))
    }
}

pub(crate) struct DbQueryHistoryWriter {
    db: Arc<AppDatabase>,
}

impl QueryHistoryWriter for DbQueryHistoryWriter {
    fn append_queries(&self, rows: Vec<AppendHistoryInput>) {
        for row in rows {
            let _ = self.db.with_conn(|c| repo::history::append(c, row));
        }
    }
}

pub(crate) fn install_writer(connections: &ConnectionManager, db: Arc<AppDatabase>) {
    connections.set_query_history_writer(Arc::new(DbQueryHistoryWriter { db }));
}
