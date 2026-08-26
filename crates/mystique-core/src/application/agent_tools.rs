//! Reusable read-only workspace tools.
//!
//! The MCP HTTP server and the in-app agent both call these methods.
//! They never return passwords, hosts, or connection strings.

mod args;
mod download;
mod specs;
mod types;

use std::sync::Arc;

use mystique_connector_api::{
    ColumnInfo, ExecuteQueryRequest, ListCatalogColumnsRequest, ListCatalogRequest,
    ListRelationshipsRequest, PoolConfig, QueryMode,
};
use serde_json::{json, Value as JsonValue};

use super::dbml::catalog_to_dbml;
use crate::connectors::{ConnectionManager, OpenSessionRequest};
use crate::domain::{SaveQueryInput, SavedQuery};
use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

use self::args::{object_info, opt_str, profile_database, req_str};
use self::download::download_from_result;
use self::types::{AGENT_DEFAULT_ROWS, AGENT_MAX_ROWS};

pub use specs::tool_specs;
pub use types::{
    AgentColumnInfo, AgentConnectionInfo, AgentObjectInfo, AgentQueryDownload, AgentToolSpec,
};

/// Shared tool surface: list schema, run read queries, save/list queries.
#[derive(Clone)]
pub struct AgentTools {
    db: Arc<AppDatabase>,
    connections: Arc<ConnectionManager>,
}

impl AgentTools {
    pub fn new(db: Arc<AppDatabase>, connections: Arc<ConnectionManager>) -> Self {
        Self { db, connections }
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
                };
                let saved = self.save_query(input)?;
                Ok(serde_json::to_value(saved)?)
            }
            other => Err(AppError::Invalid(format!("unknown tool `{other}`"))),
        }
    }

    pub async fn list_connections(&self) -> AppResult<Vec<AgentConnectionInfo>> {
        let profiles = self.db.with_conn(repo::connections::list_all)?;
        let live: std::collections::HashSet<String> = self
            .connections
            .list_sessions()
            .await
            .into_iter()
            .map(|s| s.session_id)
            .collect();
        Ok(profiles
            .into_iter()
            .map(|p| AgentConnectionInfo {
                connected: live.contains(&p.id),
                database: profile_database(&p),
                id: p.id.clone(),
                name: p.name,
                engine: p.engine,
                read_only: p.read_only,
            })
            .collect())
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
        let req = OpenSessionRequest {
            engine: profile.engine.clone(),
            plugin_config: profile.plugin_config.clone(),
            password: profile.password.clone(),
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

    pub async fn get_schema_dbml(
        &self,
        connection_id: &str,
        database: Option<&str>,
    ) -> AppResult<String> {
        let sid = self.ensure_session(connection_id).await?;
        let profile = self
            .db
            .with_conn(|c| repo::connections::get_by_id(c, &sid))?;
        let db = database
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or_else(|| profile_database(&profile));
        let columns = self
            .connections
            .list_catalog_columns(ListCatalogColumnsRequest {
                session_id: sid.clone(),
                database: db.clone(),
                schema: None,
            })
            .await?;
        let rels = self
            .connections
            .list_relationships(ListRelationshipsRequest {
                session_id: sid,
                database: db.clone(),
            })
            .await
            .unwrap_or_default();
        Ok(catalog_to_dbml(
            &profile.engine,
            &profile.name,
            db.as_deref(),
            &columns,
            &rels,
        ))
    }

    pub async fn list_objects(
        &self,
        connection_id: &str,
        database: Option<&str>,
    ) -> AppResult<Vec<AgentObjectInfo>> {
        let sid = self.ensure_session(connection_id).await?;
        let catalog = self
            .connections
            .list_catalog(ListCatalogRequest {
                session_id: sid,
                preferred_database: database
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                include_objects: true,
            })
            .await?;
        let want = database.map(str::trim).filter(|s| !s.is_empty());
        let mut out = Vec::new();
        for db in catalog {
            if let Some(w) = want {
                if db.name != w {
                    continue;
                }
            }
            for obj in db.objects {
                out.push(object_info(obj));
            }
        }
        Ok(out)
    }

    pub async fn describe_object(
        &self,
        connection_id: &str,
        name: &str,
        schema: Option<&str>,
        database: Option<&str>,
    ) -> AppResult<Vec<AgentColumnInfo>> {
        let sid = self.ensure_session(connection_id).await?;
        let cols: Vec<ColumnInfo> = self
            .connections
            .describe_table(mystique_connector_api::DescribeTableRequest {
                session_id: sid,
                table: name.trim().to_string(),
                schema: schema
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                database: database
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            })
            .await?;
        Ok(cols
            .into_iter()
            .map(|c| AgentColumnInfo {
                name: c.name,
                data_type: c.data_type,
                nullable: c.nullable,
                pk: c.pk,
            })
            .collect())
    }

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
