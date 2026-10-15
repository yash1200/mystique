use mystique_connector_api::{ExecuteQueryRequest, QueryMode, WriteRisk};

use crate::domain::{SaveQueryInput, SavedQuery};
use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repo;

use super::download::download_from_result;
use super::policy::{is_safe_dml, ToolCaller};
use super::types::{clamp_agent_max_rows, AgentCellWindow, AgentQueryDownload};
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
        window: AgentCellWindow,
        caller: ToolCaller,
    ) -> AppResult<AgentQueryDownload> {
        let sid = self.ensure_session_for(connection_id, caller).await?;
        let text = query.trim();
        if text.is_empty() {
            return Err(AppError::Invalid("query is required".into()));
        }
        let max = clamp_agent_max_rows(max_rows);
        let result = self
            .connections
            .execute_query(ExecuteQueryRequest {
                session_id: sid,
                text: text.to_string(),
                mode: QueryMode::Read,
                max_rows: Some(max),
                timeout_ms: Some(30_000),
                profile_read_only: true,
                confirm_object_name: None,
                collection: collection
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                database: database
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                execution_id: None,
            })
            .await?;
        Ok(download_from_result(result, format, window))
    }

    /// Policy-gated write (and read) query. P3.1 write guard still applies.
    pub async fn execute_query(
        &self,
        connection_id: &str,
        query: &str,
        database: Option<&str>,
        collection: Option<&str>,
        max_rows: Option<u32>,
        format: &str,
        window: AgentCellWindow,
        caller: ToolCaller,
    ) -> AppResult<AgentQueryDownload> {
        let text = query.trim();
        if text.is_empty() {
            return Err(AppError::Invalid("query is required".into()));
        }
        let profile = self
            .db
            .with_conn(|c| repo::connections::get_by_id(c, connection_id.trim()))?;
        let risk = self.classify_text(&profile.engine, text);
        let safe_dml = is_safe_dml(&profile.engine, text);
        self.enforce_write_policy(risk, safe_dml)?;
        if profile.read_only && risk != WriteRisk::Read {
            return Err(AppError::Invalid(
                "This connection is read-only. Disable read-only to run writes.".into(),
            ));
        }
        let sid = self.ensure_session_for(connection_id, caller).await?;
        let max = clamp_agent_max_rows(max_rows);
        let mode = if risk == WriteRisk::Read {
            QueryMode::Read
        } else {
            QueryMode::Write
        };
        let result = self
            .connections
            .execute_query(ExecuteQueryRequest {
                session_id: sid,
                text: text.to_string(),
                mode,
                max_rows: Some(max),
                timeout_ms: Some(30_000),
                profile_read_only: profile.read_only,
                confirm_object_name: None,
                collection: collection
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                database: database
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                execution_id: None,
            })
            .await?;
        Ok(download_from_result(result, format, window))
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

    pub(super) fn filter_saved_queries(
        &self,
        caller: ToolCaller,
        rows: Vec<SavedQuery>,
    ) -> Vec<SavedQuery> {
        if caller != ToolCaller::Mcp {
            return rows;
        }
        let allow = self.mcp_allowlist();
        rows.into_iter()
            .filter(|q| allow.contains(&q.connection_id))
            .collect()
    }
}
