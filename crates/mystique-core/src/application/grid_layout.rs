use std::sync::Arc;

use crate::connectors::GridSaveStore;
use crate::domain::{grid_table_key, GridColumnLayoutRecord, GridSaveRecord, GridSaveSummary};
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

    pub fn latest_save(
        &self,
        connection_id: &str,
        database: Option<&str>,
        schema: Option<&str>,
        table: &str,
    ) -> AppResult<Option<GridSaveSummary>> {
        let key = grid_table_key(database, schema, table);
        self.db
            .with_conn(|conn| repo::grid_layout::latest_save(conn, connection_id, &key))
    }
}

pub(crate) struct DbGridSaveStore {
    pub(crate) db: Arc<AppDatabase>,
}

impl GridSaveStore for DbGridSaveStore {
    fn put(&self, save: &GridSaveRecord) -> AppResult<()> {
        self.db
            .with_conn(|conn| repo::grid_layout::put_save(conn, save))
    }

    fn get(&self, save_id: &str) -> AppResult<Option<GridSaveRecord>> {
        self.db
            .with_conn(|conn| repo::grid_layout::get_save(conn, save_id))
    }

    fn take(&self, save_id: &str) -> AppResult<Option<GridSaveRecord>> {
        self.db
            .with_conn(|conn| repo::grid_layout::take_save(conn, save_id))
    }
}
