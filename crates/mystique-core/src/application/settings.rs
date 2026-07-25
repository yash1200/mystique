use crate::domain::AppSetting;
use crate::error::AppResult;
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

pub struct SettingsService<'a> {
    pub(crate) db: &'a AppDatabase,
}

impl SettingsService<'_> {
    pub fn get(&self, key: &str) -> AppResult<Option<AppSetting>> {
        self.db.with_conn(|c| repo::settings::get(c, key))
    }

    pub fn set(&self, key: &str, value: &str) -> AppResult<AppSetting> {
        self.db.with_conn(|c| repo::settings::set(c, key, value))
    }

    pub fn list(&self) -> AppResult<Vec<AppSetting>> {
        self.db.with_conn(repo::settings::list_all)
    }

    pub fn delete(&self, key: &str) -> AppResult<bool> {
        self.db.with_conn(|c| repo::settings::delete(c, key))
    }
}
