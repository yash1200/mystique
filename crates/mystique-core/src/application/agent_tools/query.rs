use mystique_connector_api::{ExecuteQueryRequest, QueryMode};

use crate::domain::{SaveQueryInput, SavedQuery};
use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repo;

use super::download::download_from_result;
use super::types::{AgentQueryDownload, AGENT_DEFAULT_ROWS, AGENT_MAX_ROWS};
use super::AgentTools;

impl AgentTools {
    pub async fn execute_readonly_query(
        &self,
        connection_id: &str,
        query: &str,
        database: Option<&str>,
        collection: Option<&str>,
        max_rows: Option<u32>,
        format: &str,
    ) -> AppResult<AgentQueryDownload> {
        let sid = self.ensure_session(connection_id).await?;
        let text = query.trim();
        if text.is_empty() {
            return Err(AppError::Invalid("query is required".into()));
        }
        let max = max_rows
            .unwrap_or(AGENT_DEFAULT_ROWS)
            .clamp(1, AGENT_MAX_ROWS);
        let result = self
            .connections
            .execute_query(ExecuteQueryRequest {
                session_id: sid,
                text: text.to_string(),
                mode: QueryMode::Read,
                max_rows: Some(max),
                timeout_ms: Some(30_000),
                profile_read_only: true,
                collection: collection
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                database: database
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            })
            .await?;
        Ok(download_from_result(result, format))
    }

    pub fn list_saved_queries(&self, connection_id: Option<&str>) -> AppResult<Vec<SavedQuery>> {
        match connection_id.map(str::trim).filter(|s| !s.is_empty()) {
            Some(id) => self.db.with_conn(|c| repo::queries::list_queries(c, id)),
            None => self.db.with_conn(repo::queries::list_all_queries),
        }
    }

    pub fn save_query(&self, input: SaveQueryInput) -> AppResult<SavedQuery> {
        self.db.with_conn(|c| repo::queries::save_query(c, input))
    }
}
