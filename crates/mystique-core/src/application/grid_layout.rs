use crate::domain::{AppendGridEditHistoryInput, GridColumnLayoutRecord, GridEditHistoryRecord};
use crate::error::AppResult;
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

pub struct GridLayoutService<'a> {
    pub(crate) db: &'a AppDatabase,
}

impl GridLayoutService<'_> {
    pub fn get_layout(
        &self,
        connection_id: &str,
        table_id: &str,
    ) -> AppResult<Option<GridColumnLayoutRecord>> {
        self.db
            .with_conn(|conn| repo::grid_layout::get_layout(conn, connection_id, table_id))
    }

    pub fn save_layout(&self, layout: GridColumnLayoutRecord) -> AppResult<()> {
        self.db
            .with_conn(|conn| repo::grid_layout::save_layout(conn, &layout))
    }

    pub fn latest_history(
        &self,
        connection_id: &str,
        table_id: &str,
    ) -> AppResult<Option<GridEditHistoryRecord>> {
        self.db
            .with_conn(|conn| repo::grid_layout::latest_history(conn, connection_id, table_id))
    }

    pub fn append_history(&self, input: AppendGridEditHistoryInput) -> AppResult<i64> {
        self.db
            .with_conn(|conn| repo::grid_layout::append_history(conn, &input))
    }
}
