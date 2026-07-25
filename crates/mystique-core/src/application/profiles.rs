use crate::domain::{ConnectionProfile, SaveConnectionInput};
use crate::error::AppResult;
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

pub struct ProfileService<'a> {
    pub(crate) db: &'a AppDatabase,
}

impl ProfileService<'_> {
    pub fn list(&self) -> AppResult<Vec<ConnectionProfile>> {
        self.db.with_conn(repo::connections::list_all)
    }

    pub fn get(&self, id: &str) -> AppResult<ConnectionProfile> {
        self.db.with_conn(|c| repo::connections::get_by_id(c, id))
    }

    pub fn save(&self, input: SaveConnectionInput) -> AppResult<ConnectionProfile> {
        // Future: write password to keychain, set secret_ref here.
        self.db.with_conn(|c| repo::connections::save(c, input))
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        // Future: also delete keychain entry for secret_ref.
        let id = id.trim();
        if id.is_empty() {
            return Err(crate::error::AppError::Invalid(
                "connection id is required".into(),
            ));
        }
        self.db.with_conn(|c| repo::connections::delete(c, id))
    }

    pub fn touch_connected(&self, id: &str) -> AppResult<ConnectionProfile> {
        self.db
            .with_conn(|c| repo::connections::touch_last_connected(c, id))
    }
}
