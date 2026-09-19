//! Reusable read-only workspace tools.
//!
//! The MCP HTTP server and the in-app agent both call these methods.
//! They never return passwords, hosts, or connection strings.

mod args;
mod catalog;
mod download;
mod query;
mod specs;
mod types;

use std::sync::Arc;

use mystique_connector_api::PoolConfig;
use serde_json::{json, Value as JsonValue};

use crate::connectors::{ConnectionManager, OpenSessionRequest};
use crate::domain::SaveQueryInput;
use crate::error::{AppError, AppResult};
use crate::secrets::SecretStore;
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

use self::args::{opt_str, req_str};

pub use specs::tool_specs;
pub use types::{
    AgentColumnInfo, AgentConnectionInfo, AgentObjectInfo, AgentQueryDownload, AgentToolSpec,
};

/// Shared tool surface: list schema, run read queries, save/list queries.
#[derive(Clone)]
pub struct AgentTools {
    db: Arc<AppDatabase>,
    connections: Arc<ConnectionManager>,
    secrets: Arc<SecretStore>,
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
        }
    }

    pub fn specs() -> Vec<AgentToolSpec> {
        tool_specs()
    }

    /// Dispatch by tool name. Used by MCP and (later) the in-app agent.
    pub async fn call_tool(&self, name: &str, args: &JsonValue) -> AppResult<JsonValue> {
        match name {
            "list_connections" => {
                // MCP structuredContent must be an object, not a top-level array.
                Ok(json!({ "connections": self.list_connections().await? }))
            }
            "get_schema_dbml" => {
                let id = req_str(args, "connection_id")?;
                let database = opt_str(args, "database");
                let dbml = self.get_schema_dbml(&id, database.as_deref()).await?;
                Ok(json!({ "dbml": dbml }))
            }
            "list_objects" => {
                let id = req_str(args, "connection_id")?;
                let database = opt_str(args, "database");
                let objects = self.list_objects(&id, database.as_deref()).await?;
                Ok(json!({ "objects": objects }))
            }
            "describe_object" => {
                let id = req_str(args, "connection_id")?;
                let name = req_str(args, "name")?;
                let database = opt_str(args, "database");
                let schema = opt_str(args, "schema");
                let cols = self
                    .describe_object(&id, &name, schema.as_deref(), database.as_deref())
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
                    )
                    .await?;
                Ok(serde_json::to_value(result)?)
            }
            "list_saved_queries" => {
                let id = opt_str(args, "connection_id");
                let rows = self.list_saved_queries(id.as_deref())?;
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
                let saved = self.save_query(input)?;
                Ok(serde_json::to_value(saved)?)
            }
            other => Err(AppError::Invalid(format!("unknown tool `{other}`"))),
        }
    }

    /// Open a pooled session for this profile if needed. Session id == profile id
    /// so the desktop UI can reuse it.
    pub async fn ensure_session(&self, connection_id: &str) -> AppResult<String> {
        let id = connection_id.trim();
        if id.is_empty() {
            return Err(AppError::Invalid("connection_id is required".into()));
        }
        // Reuse the desktop session. Do not ping — that is an extra network
        // round-trip and blocks the agent on a live query tab.
        if self.connections.get_session(id).await.is_some() {
            return Ok(id.to_string());
        }
        let profile = self.db.with_conn(|c| repo::connections::get_by_id(c, id))?;
        let password = super::ProfileService {
            db: &self.db,
            secrets: &self.secrets,
        }
        .resolve_password(id)?;
        let req = OpenSessionRequest {
            engine: profile.engine.clone(),
            plugin_config: profile.plugin_config.clone(),
            password,
            profile_id: Some(profile.id.clone()),
            pool: Some(PoolConfig {
                max_size: 5,
                min_idle: 0,
                acquire_timeout_ms: 10_000,
                idle_timeout_secs: 0,
            }),
            session_id: Some(profile.id.clone()),
        };
        self.connections.open_session(req).await?;
        let _ = self
            .db
            .with_conn(|c| repo::connections::touch_last_connected(c, &profile.id));
        Ok(profile.id)
    }
}
