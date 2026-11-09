//! Shared workspace tools for MCP and the in-app agent.
//!
//! They never return passwords, hosts, or connection strings. Every call is
//! checked against the caller's access level (`none < read < write < full`,
//! global → connection → database; see [`levels`]). A call above the level
//! is denied with a message the model can act on, or, when the connection
//! asks for high-risk writes, waits in the in-app approval dock.

mod approvals;
mod args;
mod catalog;
mod download;
pub mod levels;
mod policy;
mod query;
mod results;
mod specs;
mod types;

use std::sync::Arc;

use mystique_connector_api::PoolConfig;
use serde_json::{json, Value as JsonValue};

use crate::connectors::{ConnectionManager, OpenSessionRequest};
use crate::domain::SaveQueryInput;
use crate::error::{AppError, AppResult};
use crate::secrets::{Prompt, SecretStore};
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

use self::args::{opt_str, req_str};

pub(crate) use results::ToolResultCache;

pub use approvals::{ToolApprovals, APPROVAL_TIMEOUT};
pub(crate) use args::profile_database as profile_default_database;
pub use levels::{AccessLevel, ToolCaller};
pub(crate) use policy::{default_level, is_production, setting_key, ConnectionLevels};
pub use policy::{SETTING_AGENT_LEVEL, SETTING_MCP_LEVEL};
pub use specs::{tool_specs, tool_specs_for};
pub use types::{
    AgentColumnInfo, AgentConnectionInfo, AgentObjectInfo, AgentQueryDownload, AgentToolSpec,
    ToolAnnotations, AGENT_DEFAULT_CELL_CHARS, AGENT_MAX_CELL_CHARS, AGENT_MAX_ROWS,
};

/// A profile that does not save its password, with none typed this session.
pub(crate) const ASKS_EACH_SESSION: &str =
    "This connection asks for its password each session. Connect to it in Mystique first.";

/// Shared tool surface: list schema, run queries, save/list queries.
#[derive(Clone)]
pub struct AgentTools {
    db: Arc<AppDatabase>,
    connections: Arc<ConnectionManager>,
    secrets: Arc<SecretStore>,
    results: Arc<ToolResultCache>,
    approvals: Arc<ToolApprovals>,
}

impl AgentTools {
    pub fn new(
        db: Arc<AppDatabase>,
        connections: Arc<ConnectionManager>,
        secrets: Arc<SecretStore>,
    ) -> Self {
        Self {
            db,
            connections,
            secrets,
            results: Arc::new(ToolResultCache::default()),
            approvals: Arc::new(ToolApprovals::default()),
        }
    }

    /// Share one approval queue (`AppServices` owns it, the dock reads it).
    pub(crate) fn with_approvals(mut self, approvals: Arc<ToolApprovals>) -> Self {
        self.approvals = approvals;
        self
    }

    /// Share one result cache across tool calls (`AppServices` owns it).
    pub(crate) fn with_result_cache(mut self, results: Arc<ToolResultCache>) -> Self {
        self.results = results;
        self
    }

    pub fn specs() -> Vec<AgentToolSpec> {
        tool_specs()
    }

    /// `execute_query` is listed only when the caller may write somewhere.
    pub fn advertised_tools(&self, caller: ToolCaller) -> Vec<AgentToolSpec> {
        tool_specs_for(self.may_write(caller))
    }

    pub fn mcp_instructions(&self) -> String {
        "Mystique is a desktop database studio. Use list_connections first: it shows each connection's access level. The user sets levels per connection and database: read (SELECT/find), write (adds INSERT/UPDATE), full (adds DELETE, DDL, DROP). A call above the level is denied and nothing runs; the error says which level is needed. On connections that ask for high-risk writes, such a call waits up to 90 seconds for the user to approve that exact statement in Mystique. Never ask the user to paste schema.".into()
    }

    /// Dispatch by tool name. Used by MCP and the in-app agent.
    pub async fn call_tool(
        &self,
        name: &str,
        args: &JsonValue,
        caller: ToolCaller,
    ) -> AppResult<JsonValue> {
        if let Some(id) = opt_str(args, "connection_id") {
            let database = opt_str(args, "database");
            self.require_read(caller, &id, database.as_deref())?;
        }
        match name {
            "list_connections" => {
                // MCP structuredContent must be an object, not a top-level array.
                Ok(json!({ "connections": self.list_connections(caller).await? }))
            }
            "get_schema_dbml" => {
                let id = req_str(args, "connection_id")?;
                let database = opt_str(args, "database");
                let dbml = self
                    .get_schema_dbml(&id, database.as_deref(), caller)
                    .await?;
                Ok(json!({ "dbml": dbml }))
            }
            "list_objects" => {
                let id = req_str(args, "connection_id")?;
                let database = opt_str(args, "database");
                let objects = self.list_objects(&id, database.as_deref(), caller).await?;
                Ok(json!({ "objects": objects }))
            }
            "describe_object" => {
                let id = req_str(args, "connection_id")?;
                let name = req_str(args, "name")?;
                let database = opt_str(args, "database");
                let schema = opt_str(args, "schema");
                let cols = self
                    .describe_object(&id, &name, schema.as_deref(), database.as_deref(), caller)
                    .await?;
                Ok(json!({ "name": name, "schema": schema, "columns": cols }))
            }
            "execute_readonly_query" => {
                let id = req_str(args, "connection_id")?;
                let query = req_str(args, "query")?;
                let database = opt_str(args, "database");
                let collection = opt_str(args, "collection");
                let format = opt_str(args, "format").unwrap_or_else(|| "json".into());
                let max_rows = args
                    .get("max_rows")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);
                let result = self
                    .execute_readonly_query(
                        &id,
                        &query,
                        database.as_deref(),
                        collection.as_deref(),
                        max_rows,
                        &format,
                        types::AgentCellWindow::from_args(args),
                        caller,
                    )
                    .await?;
                Ok(serde_json::to_value(result)?)
            }
            "execute_query" => {
                let id = req_str(args, "connection_id")?;
                let query = req_str(args, "query")?;
                let database = opt_str(args, "database");
                let collection = opt_str(args, "collection");
                let format = opt_str(args, "format").unwrap_or_else(|| "json".into());
                let max_rows = args
                    .get("max_rows")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);
                let result = self
                    .execute_query(
                        &id,
                        &query,
                        database.as_deref(),
                        collection.as_deref(),
                        max_rows,
                        &format,
                        types::AgentCellWindow::from_args(args),
                        caller,
                    )
                    .await?;
                Ok(serde_json::to_value(result)?)
            }
            "list_saved_queries" => {
                let id = opt_str(args, "connection_id");
                let rows = self.list_saved_queries(id.as_deref())?;
                let rows = self.filter_saved_queries(caller, rows);
                Ok(json!({ "queries": rows }))
            }
            "save_query" => {
                let input = SaveQueryInput {
                    id: opt_str(args, "id"),
                    connection_id: req_str(args, "connection_id")?,
                    name: req_str(args, "name")?,
                    body: req_str(args, "body")?,
                    language: opt_str(args, "language"),
                    sort_order: args.get("sort_order").and_then(|v| v.as_i64()).unwrap_or(0),
                    favorite: args.get("favorite").and_then(|v| v.as_bool()),
                };
                // The request connection is checked above. An update also has
                // to be allowed on the row that already exists, or a scratch
                // connection can rewrite a production saved query by id.
                if let Some(id) = input.id.as_deref().filter(|s| !s.is_empty()) {
                    let existing = self.db.with_conn(|c| repo::queries::get_query(c, id))?;
                    self.require_read(caller, &existing.connection_id, None)?;
                }
                let saved = self.save_query(input)?;
                Ok(serde_json::to_value(saved)?)
            }
            other => Err(AppError::Invalid(format!("unknown tool `{other}`"))),
        }
    }

    /// Open a pooled session for this profile if needed.
    ///
    /// The id is `agent:{profile}` or `mcp:{profile}`, not the desktop session.
    /// Closing the UI pool must not cancel an in-flight tool query.
    pub async fn ensure_session(&self, connection_id: &str) -> AppResult<String> {
        self.ensure_session_for(connection_id, policy::ToolCaller::Agent)
            .await
    }

    pub async fn ensure_session_for(
        &self,
        connection_id: &str,
        caller: policy::ToolCaller,
    ) -> AppResult<String> {
        let (sid, origin) = tool_session_id(connection_id, caller)?;
        // A live pool needs no secrets: do not touch the keychain again.
        if self.connections.reuse_live(&sid).await {
            return Ok(sid);
        }
        let profile = self
            .db
            .with_conn(|c| repo::connections::get_by_id(c, connection_id.trim()))?;
        self.connections
            .set_database_marks(&profile.id, profile.database_environments.clone());
        // MCP requests arrive while nobody may be looking: never prompt.
        let prompt = match caller {
            policy::ToolCaller::Agent => Prompt::Allowed,
            policy::ToolCaller::Mcp => Prompt::Never,
        };
        let (db, secrets) = (Arc::clone(&self.db), Arc::clone(&self.secrets));
        let open = OpenSessionRequest {
            engine: profile.engine.clone(),
            plugin_config: profile.plugin_config.clone(),
            password: None,
            profile_id: Some(profile.id.clone()),
            pool: Some(PoolConfig {
                max_size: 5,
                min_idle: 0,
                acquire_timeout_ms: 10_000,
                idle_timeout_secs: 0,
            }),
            session_id: Some(sid.clone()),
            ssh: None,
            tls_key_passphrase: None,
            environment: Some(profile.environment.clone()),
            connection_name: Some(profile.name.clone()),
            profile_read_only: profile.read_only,
            use_saved_ssh: true,
        };
        // The vault key may load here: off the async workers.
        let (req, session) = tokio::task::spawn_blocking(move || {
            super::bind_with_session(&db, &secrets, open, prompt)
        })
        .await
        .map_err(|e| AppError::Internal(format!("secret binding: {e}")))?
        .map_err(|e| match e {
            // Tools cannot ask. The desktop connect fills the session store.
            AppError::PasswordRequired => AppError::SecretStore(ASKS_EACH_SESSION.into()),
            other => other,
        })?;
        let opened = self.connections.get_or_open(req, origin).await;
        super::settle_session_password(&self.secrets, session, &opened);
        opened?;
        let _ = self
            .db
            .with_conn(|c| repo::connections::touch_last_connected(c, &profile.id));
        Ok(sid)
    }
}

/// `agent:{profile}` or `mcp:{profile}`, never the desktop session.
fn tool_session_id(
    connection_id: &str,
    caller: policy::ToolCaller,
) -> AppResult<(String, crate::connectors::SessionOrigin)> {
    let profile_id = connection_id.trim();
    if profile_id.is_empty() {
        return Err(AppError::Invalid("connection_id is required".into()));
    }
    Ok(match caller {
        policy::ToolCaller::Agent => (
            format!("agent:{profile_id}"),
            crate::connectors::SessionOrigin::Agent,
        ),
        policy::ToolCaller::Mcp => (
            format!("mcp:{profile_id}"),
            crate::connectors::SessionOrigin::Mcp,
        ),
    })
}

#[cfg(test)]
mod access_tests;

#[cfg(test)]
mod principal_tests;
