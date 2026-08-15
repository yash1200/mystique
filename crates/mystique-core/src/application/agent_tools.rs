//! Reusable read-only workspace tools.
//!
//! The MCP HTTP server and the future in-app agent both call these methods.
//! They never return passwords, hosts, or connection strings.

use std::sync::Arc;

use mystique_connector_api::{
    CatalogObject, ColumnInfo, ExecuteQueryRequest, ListCatalogColumnsRequest, ListCatalogRequest,
    ListRelationshipsRequest, PoolConfig, QueryMode, QueryResult,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use super::dbml::catalog_to_dbml;
use crate::connectors::{ConnectionManager, OpenSessionRequest};
use crate::domain::{ConnectionProfile, SaveQueryInput, SavedQuery};
use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

pub const AGENT_MAX_ROWS: u32 = 2_000;
pub const AGENT_DEFAULT_ROWS: u32 = 200;

/// Safe connection card for models (no host / password / URI).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConnectionInfo {
    pub id: String,
    pub name: String,
    pub engine: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub read_only: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentObjectInfo {
    pub name: String,
    /// Schema-qualified name for SQL (`main.boards`). Same as `name` when schema is public/empty.
    pub qualified_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub row_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub pk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentQueryDownload {
    pub engine: String,
    pub columns: Vec<String>,
    pub row_count: u64,
    pub truncated: bool,
    pub duration_ms: u64,
    /// `json` rows or a CSV string.
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<Vec<JsonValue>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub csv: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: JsonValue,
}

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
        if self.connections.get_session(id).await.is_some()
            && self.connections.ping(id).await.is_ok()
        {
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

fn download_from_result(result: QueryResult, format: &str) -> AgentQueryDownload {
    let columns: Vec<String> = result.columns.iter().map(|c| c.name.clone()).collect();
    let csv_wanted = format.eq_ignore_ascii_case("csv");
    if csv_wanted {
        AgentQueryDownload {
            engine: result.engine,
            columns,
            row_count: result.row_count,
            truncated: result.truncated,
            duration_ms: result.duration_ms,
            format: "csv".into(),
            rows: None,
            csv: Some(rows_to_csv(
                &result
                    .columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>(),
                &result.rows,
            )),
        }
    } else {
        AgentQueryDownload {
            engine: result.engine,
            columns,
            row_count: result.row_count,
            truncated: result.truncated,
            duration_ms: result.duration_ms,
            format: "json".into(),
            rows: Some(result.rows),
            csv: None,
        }
    }
}

fn rows_to_csv(headers: &[&str], rows: &[Vec<JsonValue>]) -> String {
    let mut out = String::new();
    out.push_str(
        &headers
            .iter()
            .map(|h| csv_escape(h))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');
    for row in rows {
        let line: Vec<String> = (0..headers.len())
            .map(|i| {
                let cell = row.get(i).unwrap_or(&JsonValue::Null);
                csv_escape(&json_cell(cell))
            })
            .collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

fn json_cell(v: &JsonValue) -> String {
    match v {
        JsonValue::Null => String::new(),
        JsonValue::String(s) => s.clone(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn object_info(obj: CatalogObject) -> AgentObjectInfo {
    let schema = obj.schema.clone().unwrap_or_default();
    let qualified_name = super::dbml::table_ident(&schema, &obj.name);
    AgentObjectInfo {
        name: obj.name,
        qualified_name,
        schema: obj.schema,
        database: obj.database,
        kind: obj.kind,
        row_count: obj.row_count,
    }
}

fn profile_database(profile: &ConnectionProfile) -> Option<String> {
    let cfg = &profile.plugin_config;
    if let Some(s) = cfg
        .get("database")
        .or_else(|| cfg.get("db"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(s.to_string());
    }
    if let Some(path) = cfg
        .get("path")
        .or_else(|| cfg.get("file"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(path.to_string());
    }
    if let Some(uri) = cfg.get("uri").and_then(|v| v.as_str()) {
        if let Some(db) = database_from_mongo_uri(uri) {
            return Some(db);
        }
    }
    None
}

fn database_from_mongo_uri(uri: &str) -> Option<String> {
    let after = uri.split("://").nth(1)?;
    let path = after.split_once('/')?.1;
    let name = path.split('?').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn req_str(args: &JsonValue, key: &str) -> AppResult<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Invalid(format!("`{key}` is required")))
}

fn opt_str(args: &JsonValue, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

pub fn tool_specs() -> Vec<AgentToolSpec> {
    vec![
        AgentToolSpec {
            name: "list_connections",
            description:
                "List saved Mystique DB connections: id, name, engine, database. No hosts or passwords.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        AgentToolSpec {
            name: "get_schema_dbml",
            description:
                "Return live DBML (tables/collections, columns, keys, relationships) for a connection. Opens the connection if needed. Use schema-qualified names from the Table headers (e.g. main.boards) — unqualified names fail when the table is not on search_path.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string", "description": "Connection profile id from list_connections" },
                    "database": { "type": "string", "description": "Optional database / catalog name" }
                },
                "required": ["connection_id"],
                "additionalProperties": false
            }),
        },
        AgentToolSpec {
            name: "list_objects",
            description:
                "List tables, views, or collections in a connection (opens it if needed). Use qualified_name in SQL (e.g. main.boards).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "database": { "type": "string" }
                },
                "required": ["connection_id"],
                "additionalProperties": false
            }),
        },
        AgentToolSpec {
            name: "describe_object",
            description:
                "Describe columns / fields of one table or collection.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "name": { "type": "string", "description": "Table or collection name" },
                    "schema": { "type": "string" },
                    "database": { "type": "string" }
                },
                "required": ["connection_id", "name"],
                "additionalProperties": false
            }),
        },
        AgentToolSpec {
            name: "execute_readonly_query",
            description:
                "Run a read-only query (SELECT / find). Writes are rejected. Returns JSON rows or CSV for download. Opens the connection if needed. Qualify tables with schema (SELECT * FROM main.boards), not bare names.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "query": { "type": "string", "description": "SQL or Mongo find/aggregate / JSON filter" },
                    "database": { "type": "string" },
                    "collection": { "type": "string", "description": "Mongo collection when required" },
                    "max_rows": { "type": "integer", "minimum": 1, "maximum": 2000 },
                    "format": { "type": "string", "enum": ["json", "csv"], "description": "json (default) or csv" }
                },
                "required": ["connection_id", "query"],
                "additionalProperties": false
            }),
        },
        AgentToolSpec {
            name: "list_saved_queries",
            description: "List queries saved in Mystique DB. Optionally filter by connection_id.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" }
                },
                "additionalProperties": false
            }),
        },
        AgentToolSpec {
            name: "save_query",
            description: "Save a named query on a connection so it appears in Mystique DB.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "name": { "type": "string" },
                    "body": { "type": "string", "description": "Query text" },
                    "language": { "type": "string", "description": "sql | mql" },
                    "id": { "type": "string", "description": "Existing id to update" }
                },
                "required": ["connection_id", "name", "body"],
                "additionalProperties": false
            }),
        },
    ]
}
