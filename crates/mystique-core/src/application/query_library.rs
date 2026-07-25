use crate::domain::{
    QueryGroup, SaveQueryGroupInput, SaveQueryInput, SavedQuery,
};
use crate::error::AppResult;
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

pub struct QueryLibraryService<'a> {
    pub(crate) db: &'a AppDatabase,
}

impl QueryLibraryService<'_> {
    pub fn list_groups(&self, connection_id: &str) -> AppResult<Vec<QueryGroup>> {
        self.db
            .with_conn(|c| repo::queries::list_groups(c, connection_id))
    }

    pub fn save_group(&self, input: SaveQueryGroupInput) -> AppResult<QueryGroup> {
        self.db.with_conn(|c| repo::queries::save_group(c, input))
    }

    pub fn delete_group(&self, id: &str) -> AppResult<()> {
        self.db.with_conn(|c| repo::queries::delete_group(c, id))
    }

    pub fn list_queries(&self, connection_id: &str) -> AppResult<Vec<SavedQuery>> {
        self.db
            .with_conn(|c| repo::queries::list_queries(c, connection_id))
    }

    pub fn save_query(&self, input: SaveQueryInput) -> AppResult<SavedQuery> {
        self.db.with_conn(|c| repo::queries::save_query(c, input))
    }

    pub fn delete_query(&self, id: &str) -> AppResult<()> {
        self.db.with_conn(|c| repo::queries::delete_query(c, id))
    }
}
