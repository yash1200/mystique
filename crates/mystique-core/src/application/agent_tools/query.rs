use mystique_connector_api::{ExecuteQueryRequest, QueryMode, QueryResult, WriteRisk};

use crate::domain::{SaveQueryInput, SavedQuery};
use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repo;

use super::download::download_from_result;
use super::policy::ToolCaller;
use super::results::ResultKey;
use super::types::{clamp_agent_max_rows, AgentCellWindow, AgentQueryDownload};
use super::{tool_session_id, AgentTools};

fn trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

impl AgentTools {
    // Mirrors the query tool's arguments one to one (MCP and the in-app agent).
    #[allow(clippy::too_many_arguments)]
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
        let text = query.trim();
        if text.is_empty() {
            return Err(AppError::Invalid("query is required".into()));
        }
        let profile = self.require_read(caller, connection_id, database)?;
        // Looks like a read but ReadProof cannot prove it: say where it can run.
        if let Some(why) = self.assess(&profile.engine, text).unproven {
            return Err(AppError::AccessDenied(format!(
                "execute_readonly_query runs only statements Mystique can prove are reads, and this one is not: {why} Nothing ran. Use execute_query instead; it runs this in Write mode and needs `write` access on the connection."
            )));
        }
        let key = self.result_key(connection_id, caller, text, database, collection, max_rows)?;
        if let Some(hit) = self.cached_page(&key, window, true)? {
            return Ok(download_from_result(hit, format, window));
        }
        let sid = self.ensure_session_for(connection_id, caller).await?;
        let result = self
            .connections
            .execute_query(ExecuteQueryRequest {
                session_id: sid,
                text: text.to_string(),
                mode: QueryMode::Read,
                max_rows: Some(key.max_rows),
                timeout_ms: Some(30_000),
                confirm_object_name: None,
                collection: key.collection.clone(),
                database: key.database.clone(),
                execution_id: None,
                params: Vec::new(),
            })
            .await?;
        self.results.put(key, &result);
        Ok(download_from_result(result, format, window))
    }

    /// A query checked against the caller's access level (F-02). A write runs
    /// on the tool's own session with a one-shot authorization for exactly
    /// this text (F-01); no human unlock or pinned tab applies.
    // Mirrors the query tool's arguments one to one (MCP and the in-app agent).
    #[allow(clippy::too_many_arguments)]
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
        let profile = self.require_read(caller, connection_id, database)?;
        // ReadProof passes: Read mode. Otherwise Write mode, at the level
        // the statement needs (an unprovable read needs `write`).
        let assessed = self.assess(&profile.engine, text);
        let risk = assessed.risk;
        // Read-only on the profile wins for every principal, before any ask.
        if profile.read_only && risk != WriteRisk::Read {
            return Err(AppError::AccessDenied(format!(
                "Access denied: connection \"{}\" is read-only in Mystique, so writes are blocked for everyone. Nothing ran. Use execute_readonly_query, or ask the user to turn off read-only on the connection.",
                profile.name
            )));
        }
        let key = self.result_key(connection_id, caller, text, database, collection, max_rows)?;
        // A page of a result this caller already ran: nothing runs again.
        if let Some(hit) = self.cached_page(&key, window, risk == WriteRisk::Read)? {
            return Ok(download_from_result(hit, format, window));
        }
        // Spent here: approving this statement covers this one run only.
        let _grant = self
            .authorize_statement(caller, &profile, database, text, assessed.need)
            .await
            .map_err(|err| with_read_proof_reason(err, assessed.unproven.as_deref()))?;
        let sid = self.ensure_session_for(connection_id, caller).await?;
        let _write =
            (risk != WriteRisk::Read).then(|| self.connections.authorize_tool_write(&sid, text));
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
                max_rows: Some(key.max_rows),
                timeout_ms: Some(30_000),
                confirm_object_name: None,
                collection: key.collection.clone(),
                database: key.database.clone(),
                execution_id: None,
                params: Vec::new(),
            })
            .await?;
        self.results.put(key, &result);
        Ok(download_from_result(result, format, window))
    }

    fn result_key(
        &self,
        connection_id: &str,
        caller: ToolCaller,
        text: &str,
        database: Option<&str>,
        collection: Option<&str>,
        max_rows: Option<u32>,
    ) -> AppResult<ResultKey> {
        let (session_id, _) = tool_session_id(connection_id, caller)?;
        Ok(ResultKey {
            session_id,
            query: text.to_string(),
            database: trimmed(database),
            collection: trimmed(collection),
            max_rows: clamp_agent_max_rows(max_rows),
        })
    }

    /// A page past offset 0 comes from the cached result. A read may run
    /// again when the cache has expired; a write never does.
    fn cached_page(
        &self,
        key: &ResultKey,
        window: AgentCellWindow,
        is_read: bool,
    ) -> AppResult<Option<QueryResult>> {
        if window.offset == 0 {
            return Ok(None);
        }
        if let Some(hit) = self.results.get(key) {
            return Ok(Some(hit));
        }
        if is_read {
            return Ok(None);
        }
        Err(AppError::Invalid(
            "cell_char_offset pages a cached result, and this statement's result is no longer cached. A write is never run again to page it; SELECT the value instead.".into(),
        ))
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

    /// Only queries on connections the caller can see.
    pub(super) fn filter_saved_queries(
        &self,
        caller: ToolCaller,
        rows: Vec<SavedQuery>,
    ) -> Vec<SavedQuery> {
        let mut seen = std::collections::HashMap::new();
        rows.into_iter()
            .filter(|q| {
                *seen
                    .entry(q.connection_id.clone())
                    .or_insert_with(|| self.is_visible(caller, &q.connection_id))
            })
            .collect()
    }
}

/// A denied unprovable read says why it is not treated as a read.
fn with_read_proof_reason(err: AppError, unproven: Option<&str>) -> AppError {
    match (err, unproven) {
        (AppError::AccessDenied(msg), Some(why)) => AppError::AccessDenied(format!(
            "{msg} It is not run as a read because Mystique cannot prove it only reads: {why}"
        )),
        (err, _) => err,
    }
}
