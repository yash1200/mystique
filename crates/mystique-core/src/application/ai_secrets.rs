use crate::domain::AiSecretBlob;
use crate::error::AppResult;
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

pub struct AiSecretsService<'a> {
    pub(crate) db: &'a AppDatabase,
}

impl AiSecretsService<'_> {
    pub fn get(&self, provider: &str) -> AppResult<Option<AiSecretBlob>> {
        self.db.with_conn(|c| repo::ai_secrets::get(c, provider))
    }

    pub fn set(&self, provider: &str, ciphertext: &str) -> AppResult<AiSecretBlob> {
        self.db
            .with_conn(|c| repo::ai_secrets::set(c, provider, ciphertext))
    }

    pub fn delete(&self, provider: &str) -> AppResult<bool> {
        self.db.with_conn(|c| repo::ai_secrets::delete(c, provider))
    }

    pub fn list_providers(&self) -> AppResult<Vec<String>> {
        self.db.with_conn(repo::ai_secrets::list_providers)
    }
}
