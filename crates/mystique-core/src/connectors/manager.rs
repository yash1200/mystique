use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use mystique_connector_api::{
    ApplyTableEditsInput, ApplyTableEditsRequest, ApplyTableEditsResult, CatalogColumnRow,
    CatalogDatabase, CatalogRelationship, ColumnInfo, ConnectionConfig, ConnectorError,
    ConnectorResult, ConnectorSession, DescribeTableRequest, EngineDescriptor, ExecuteQueryRequest,
    ExportObjectRequest, ExportObjectResult, FetchTablePreviewRequest, ImportObjectRequest,
    ImportObjectResult, ListCatalogColumnsRequest, ListCatalogRequest, ListRelationshipsRequest,
    FormattedConnectionUrl, ParsedConnectionUrl, PoolConfig, PoolStatus, PreviewImportRequest,
    PreviewImportResult, QueryMode, QueryResult, SessionInfo, TableAdminOp, TableAdminRequest,
    TableAdminResult, TablePreviewResult, TestResult, ValidatedQuery, DEFAULT_MAX_ROWS,
    DEFAULT_TIMEOUT_MS, HARD_MAX_ROWS,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::ConnectorRegistry;

/// API request to open a pooled session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionRequest {
    pub engine: String,
    pub plugin_config: JsonValue,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub pool: Option<PoolConfig>,
    /// Optional client-supplied id (defaults to UUID).
    #[serde(default)]
    pub session_id: Option<String>,
}

struct LiveSession {
    session: Arc<dyn ConnectorSession>,
    display_name: String,
    opened_at: String,
}

/// Tracks open engine sessions (each session owns its connection pool).
pub struct ConnectionManager {
    registry: ConnectorRegistry,
    sessions: RwLock<HashMap<String, LiveSession>>,
}

impl ConnectionManager {
    pub fn new(registry: ConnectorRegistry) -> Self {
        Self {
            registry,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub fn bootstrap() -> Self {
        Self::new(ConnectorRegistry::bootstrap())
    }

    pub fn registry(&self) -> &ConnectorRegistry {
        &self.registry
    }

    pub fn list_engines(&self) -> Vec<EngineDescriptor> {
        self.registry.list()
    }

    /// Resolve connector for URL helpers (`mariadb` → mysql plugin).
    fn resolve_url_connector(
        &self,
        engine: &str,
    ) -> ConnectorResult<std::sync::Arc<dyn mystique_connector_api::DatabaseConnector>> {
        let engine_key = engine.trim().to_ascii_lowercase();
        if engine_key.is_empty() {
            return Err(ConnectorError::InvalidConfig("engine is required".into()));
        }
        let engine_id = if engine_key == "mariadb" {
            "mysql"
        } else {
            engine_key.as_str()
        };
        self.registry.get(engine_id).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("unknown engine `{engine}`"))
        })
    }

    /// Parse a connection URL via the engine plugin’s driver (host/port/db/user/ssl).
    ///
    /// `mariadb` is resolved to the MySQL plugin (same wire protocol / URL shape).
    pub fn parse_connection_url(
        &self,
        engine: &str,
        url: &str,
    ) -> ConnectorResult<ParsedConnectionUrl> {
        let connector = self.resolve_url_connector(engine)?;
        connector.parse_connection_url(url)
    }

    /// Format discrete plugin config into a connection URL via the engine plugin.
    pub fn format_connection_url(
        &self,
        engine: &str,
        plugin_config: JsonValue,
        password: Option<String>,
    ) -> ConnectorResult<FormattedConnectionUrl> {
        let connector = self.resolve_url_connector(engine)?;
        let config = ConnectionConfig {
            engine: connector.id().to_string(),
            plugin_config,
            password,
            pool: PoolConfig::default(),
        };
        connector.format_connection_url(&config)
    }

    /// Always returns a structured [`TestResult`] (never fails the API envelope for connect errors).
    /// `ok: true` means the engine accepted the credentials and answered a ping/version probe.
    pub async fn test_connection(&self, req: &OpenSessionRequest) -> ConnectorResult<TestResult> {
        let start = std::time::Instant::now();
        let connector = match self.registry.get(&req.engine) {
            Some(c) => c,
            None => {
                return Ok(TestResult {
                    ok: false,
                    engine: req.engine.clone(),
                    server_version: None,
                    latency_ms: start.elapsed().as_millis() as u64,
                    message: format!("Unknown engine `{}`", req.engine),
                });
            }
        };

        let config = ConnectionConfig {
            engine: req.engine.clone(),
            plugin_config: req.plugin_config.clone(),
            password: req.password.clone(),
            pool: req.pool.clone().unwrap_or_default(),
        };

        match connector.test_connection(&config).await {
            Ok(mut result) => {
                // Ensure latency is populated even if plugin left it at 0
                if result.latency_ms == 0 {
                    result.latency_ms = start.elapsed().as_millis() as u64;
                }
                Ok(result)
            }
            Err(e) => Ok(TestResult {
                ok: false,
                engine: req.engine.clone(),
                server_version: None,
                latency_ms: start.elapsed().as_millis() as u64,
                message: e.to_string(),
            }),
        }
    }

    pub async fn open_session(&self, req: OpenSessionRequest) -> ConnectorResult<SessionInfo> {
        let connector = self.registry.get(&req.engine).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("unknown engine `{}`", req.engine))
        })?;

        let config = ConnectionConfig {
            engine: req.engine.clone(),
            plugin_config: req.plugin_config.clone(),
            password: req.password.clone(),
            pool: req.pool.clone().unwrap_or_default(),
        };

        let session = connector.connect(&config).await?;
        let session: Arc<dyn ConnectorSession> = Arc::from(session);
        let session_id = req
            .session_id
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let info = SessionInfo {
            session_id: session_id.clone(),
            engine: session.engine_id().to_string(),
            display_name: connector.display_name().to_string(),
            capabilities: session.capabilities(),
            pool: session.pool_status(),
            opened_at: Utc::now().to_rfc3339(),
        };

        let live = LiveSession {
            session,
            display_name: info.display_name.clone(),
            opened_at: info.opened_at.clone(),
        };

        // Replace previous session with same id if any
        let mut guard = self.sessions.write().await;
        if let Some(prev) = guard.remove(&session_id) {
            let _ = prev.session.close().await;
        }
        guard.insert(session_id, live);
        Ok(info)
    }

    pub async fn close_session(&self, session_id: &str) -> ConnectorResult<()> {
        let mut guard = self.sessions.write().await;
        if let Some(live) = guard.remove(session_id) {
            live.session.close().await?;
            Ok(())
        } else {
            Err(ConnectorError::InvalidConfig(format!(
                "session not found: {session_id}"
            )))
        }
    }

    pub async fn ping(&self, session_id: &str) -> ConnectorResult<()> {
        let guard = self.sessions.read().await;
        let live = guard.get(session_id).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("session not found: {session_id}"))
        })?;
        live.session.ping().await
    }

    pub async fn pool_status(&self, session_id: &str) -> ConnectorResult<Option<PoolStatus>> {
        let guard = self.sessions.read().await;
        let live = guard.get(session_id).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("session not found: {session_id}"))
        })?;
        Ok(live.session.pool_status())
    }

    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let guard = self.sessions.read().await;
        guard
            .iter()
            .map(|(id, live)| SessionInfo {
                session_id: id.clone(),
                engine: live.session.engine_id().to_string(),
                display_name: live.display_name.clone(),
                capabilities: live.session.capabilities(),
                pool: live.session.pool_status(),
                opened_at: live.opened_at.clone(),
            })
            .collect()
    }

    pub async fn get_session(&self, session_id: &str) -> Option<SessionInfo> {
        let guard = self.sessions.read().await;
        guard.get(session_id).map(|live| SessionInfo {
            session_id: session_id.to_string(),
            engine: live.session.engine_id().to_string(),
            display_name: live.display_name.clone(),
            capabilities: live.session.capabilities(),
            pool: live.session.pool_status(),
            opened_at: live.opened_at.clone(),
        })
    }
    /// Validate via per-engine policy SPI, then execute on the live session.
    pub async fn execute_query(&self, req: ExecuteQueryRequest) -> ConnectorResult<QueryResult> {
        let session = {
            let guard = self.sessions.read().await;
            let live = guard.get(&req.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", req.session_id))
            })?;
            Arc::clone(&live.session)
        };

        let engine = session.engine_id().to_string();
        let mode = if req.profile_read_only {
            QueryMode::Read
        } else {
            req.mode
        };

        let policy = self.registry.policy(&engine).ok_or_else(|| {
            ConnectorError::Internal(format!("no query policy registered for engine `{engine}`"))
        })?;
        policy.validate(&req.text, mode)?;

        let max_rows = req
            .max_rows
            .unwrap_or(DEFAULT_MAX_ROWS)
            .clamp(1, HARD_MAX_ROWS);
        let timeout_ms = req.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS).max(1_000);

        let validated = ValidatedQuery {
            text: req.text,
            mode,
            max_rows,
            timeout_ms,
            collection: req.collection,
            database: req.database,
        };

        session.execute_query(&validated).await
    }

    /// Column metadata with plugin-owned type labels (e.g. `timestamptz`).
    pub async fn describe_table(
        &self,
        req: DescribeTableRequest,
    ) -> ConnectorResult<Vec<ColumnInfo>> {
        let session = {
            let guard = self.sessions.read().await;
            let live = guard.get(&req.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", req.session_id))
            })?;
            Arc::clone(&live.session)
        };
        session
            .describe_table(req.database.as_deref(), req.schema.as_deref(), &req.table)
            .await
    }

    /// Bulk column listing for ER / catalog (plugin-normalized types).
    pub async fn list_catalog_columns(
        &self,
        req: ListCatalogColumnsRequest,
    ) -> ConnectorResult<Vec<CatalogColumnRow>> {
        let session = {
            let guard = self.sessions.read().await;
            let live = guard.get(&req.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", req.session_id))
            })?;
            Arc::clone(&live.session)
        };
        session
            .list_catalog_columns(req.database.as_deref(), req.schema.as_deref())
            .await
    }

    /// Persist grid edits via the engine plugin (dialect + execution).
    pub async fn apply_table_edits(
        &self,
        req: ApplyTableEditsRequest,
    ) -> ConnectorResult<ApplyTableEditsResult> {
        if req.profile_read_only {
            return Err(ConnectorError::Policy(
                "This connection is read-only. Disable read-only to save edits.".into(),
            ));
        }
        let session = {
            let guard = self.sessions.read().await;
            let live = guard.get(&req.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", req.session_id))
            })?;
            Arc::clone(&live.session)
        };
        if req.updates.is_empty() && req.inserts.is_empty() && req.deletes.is_empty() {
            return Ok(ApplyTableEditsResult::empty());
        }
        let input = ApplyTableEditsInput::from(req);
        session.apply_table_edits(&input).await
    }

    pub async fn fetch_table_preview(
        &self,
        req: FetchTablePreviewRequest,
    ) -> ConnectorResult<TablePreviewResult> {
        let session = {
            let guard = self.sessions.read().await;
            let live = guard.get(&req.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", req.session_id))
            })?;
            Arc::clone(&live.session)
        };
        let input = req.into_input(DEFAULT_MAX_ROWS, HARD_MAX_ROWS);
        session.fetch_table_preview(&input).await
    }

    pub async fn list_catalog(
        &self,
        req: ListCatalogRequest,
    ) -> ConnectorResult<Vec<CatalogDatabase>> {
        let session = {
            let guard = self.sessions.read().await;
            let live = guard.get(&req.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", req.session_id))
            })?;
            Arc::clone(&live.session)
        };
        session
            .list_catalog(req.preferred_database.as_deref(), req.include_objects)
            .await
    }

    pub async fn list_relationships(
        &self,
        req: ListRelationshipsRequest,
    ) -> ConnectorResult<Vec<CatalogRelationship>> {
        let session = {
            let guard = self.sessions.read().await;
            let live = guard.get(&req.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", req.session_id))
            })?;
            Arc::clone(&live.session)
        };
        session.list_relationships(req.database.as_deref()).await
    }

    /// Plugin-owned structure (columns, indexes, triggers, relations, …).
    pub async fn describe_object_structure(
        &self,
        req: crate::ObjectStructureRequest,
    ) -> ConnectorResult<crate::ObjectStructure> {
        if req.table.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Object name is required.".into(),
            ));
        }
        let session = {
            let guard = self.sessions.read().await;
            let live = guard.get(&req.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", req.session_id))
            })?;
            Arc::clone(&live.session)
        };
        session
            .describe_object_structure(
                req.database.as_deref(),
                req.schema.as_deref(),
                &req.table,
                req.kind.as_deref(),
            )
            .await
    }

    /// Plugin-owned table/collection admin (CREATE text + mutating ops).
    pub async fn apply_table_admin(
        &self,
        req: TableAdminRequest,
    ) -> ConnectorResult<TableAdminResult> {
        if req.profile_read_only && req.op != TableAdminOp::CreateStatement {
            return Err(ConnectorError::Policy(
                "This connection is read-only. Disable read-only to change objects.".into(),
            ));
        }
        let needs_name = matches!(req.op, TableAdminOp::Rename | TableAdminOp::Duplicate);
        if needs_name
            && req
                .new_name
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        {
            return Err(ConnectorError::InvalidConfig(
                "newName is required for rename and duplicate.".into(),
            ));
        }
        if req.name.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Object name is required.".into(),
            ));
        }
        if matches!(
            req.op,
            TableAdminOp::CreateGroup | TableAdminOp::CreateObject
        ) && !req
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(ConnectorError::InvalidConfig(
                "Name may only contain letters, numbers, and underscore.".into(),
            ));
        }
        let session = {
            let guard = self.sessions.read().await;
            let live = guard.get(&req.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", req.session_id))
            })?;
            Arc::clone(&live.session)
        };
        let input = req.into_input();
        session.apply_table_admin(&input).await
    }

    /// Plugin-owned full-table file export (100-row chunks, no row cap).
    pub async fn export_object(
        &self,
        req: ExportObjectRequest,
    ) -> ConnectorResult<ExportObjectResult> {
        if req.name.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Object name is required.".into(),
            ));
        }
        if req.path.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Export path is required.".into(),
            ));
        }
        let session = {
            let guard = self.sessions.read().await;
            let live = guard.get(&req.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", req.session_id))
            })?;
            Arc::clone(&live.session)
        };
        let allowed = session.capabilities().export_formats;
        let format = req.format.as_str();
        if !allowed.iter().any(|f| f == format) {
            return Err(ConnectorError::NotSupported(format!(
                "this engine does not export {format} files"
            )));
        }
        session.export_object(&req.into_input()).await
    }

    pub async fn preview_import(
        &self,
        req: PreviewImportRequest,
    ) -> ConnectorResult<PreviewImportResult> {
        if req.name.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Object name is required.".into(),
            ));
        }
        if req.path.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Import path is required.".into(),
            ));
        }
        let session = {
            let guard = self.sessions.read().await;
            let live = guard.get(&req.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", req.session_id))
            })?;
            Arc::clone(&live.session)
        };
        let allowed = session.capabilities().export_formats;
        let format = req.format.as_str();
        if !allowed.iter().any(|f| f == format) {
            return Err(ConnectorError::NotSupported(format!(
                "this engine does not import {format} files"
            )));
        }
        session.preview_import(&req.into_input()).await
    }

    pub async fn import_object(
        &self,
        req: ImportObjectRequest,
    ) -> ConnectorResult<ImportObjectResult> {
        if req.profile_read_only {
            return Err(ConnectorError::Policy(
                "This connection is read-only. Disable read-only to import data.".into(),
            ));
        }
        if req.name.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Object name is required.".into(),
            ));
        }
        if req.path.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Import path is required.".into(),
            ));
        }
        let session = {
            let guard = self.sessions.read().await;
            let live = guard.get(&req.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", req.session_id))
            })?;
            Arc::clone(&live.session)
        };
        let allowed = session.capabilities().export_formats;
        let format = req.format.as_str();
        if !allowed.iter().any(|f| f == format) {
            return Err(ConnectorError::NotSupported(format!(
                "this engine does not import {format} files"
            )));
        }
        session.import_object(&req.into_input()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_connection_url_postgres_and_mysql() {
        let mgr = ConnectionManager::bootstrap();
        let pg = mgr
            .parse_connection_url(
                "postgres",
                "postgresql://app:s3cret@db.example.com:5433/appdb?sslmode=require",
            )
            .expect("postgres url");
        assert_eq!(pg.plugin_config["host"], "db.example.com");
        assert_eq!(pg.plugin_config["port"], 5433);
        assert_eq!(pg.plugin_config["database"], "appdb");
        assert_eq!(pg.password.as_deref(), Some("s3cret"));
        assert_eq!(pg.plugin_config["ssl"], true);

        let my = mgr
            .parse_connection_url("mysql", "mysql://admin:123@127.0.0.1:3307/mystique")
            .expect("mysql url");
        assert_eq!(my.plugin_config["host"], "127.0.0.1");
        assert_eq!(my.plugin_config["port"], 3307);
        assert_eq!(my.plugin_config["username"], "admin");
        assert_eq!(my.password.as_deref(), Some("123"));

        // MariaDB alias → mysql plugin
        let md = mgr
            .parse_connection_url("mariadb", "mariadb://root@localhost/db")
            .expect("mariadb url");
        assert_eq!(md.engine, "mysql");
        assert_eq!(md.plugin_config["database"], "db");
    }

    #[test]
    fn format_connection_url_postgres_and_mysql() {
        let mgr = ConnectionManager::bootstrap();
        let pg = mgr
            .format_connection_url(
                "postgres",
                json!({
                    "host": "db.example.com",
                    "port": 5433,
                    "database": "appdb",
                    "username": "app",
                    "ssl": true,
                    "sslMode": "require"
                }),
                Some("s3cret".into()),
            )
            .expect("format postgres");
        assert!(
            pg.url.contains("postgresql://") && pg.url.contains("db.example.com:5433"),
            "url={}",
            pg.url
        );
        assert!(pg.url.contains("sslmode=require"), "url={}", pg.url);

        let my = mgr
            .format_connection_url(
                "mysql",
                json!({
                    "host": "127.0.0.1",
                    "port": 3306,
                    "database": "mystique",
                    "username": "admin"
                }),
                Some("123".into()),
            )
            .expect("format mysql");
        assert!(my.url.starts_with("mysql://"), "url={}", my.url);
        assert!(my.url.contains("admin"), "url={}", my.url);

        let mongo = mgr
            .format_connection_url(
                "mongodb",
                json!({ "uri": "mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin" }),
                None,
            )
            .expect("format mongo");
        assert!(mongo.url.starts_with("mongodb://"), "url={}", mongo.url);

        let sqlite = mgr
            .format_connection_url(
                "sqlite",
                json!({ "path": "/tmp/app.sqlite" }),
                None,
            )
            .expect("format sqlite");
        assert_eq!(sqlite.url, "file:///tmp/app.sqlite");
    }

    #[tokio::test]
    async fn test_unknown_engine_returns_structured_fail() {
        let mgr = ConnectionManager::bootstrap();
        let result = mgr
            .test_connection(&OpenSessionRequest {
                engine: "nope".into(),
                plugin_config: json!({}),
                password: None,
                pool: None,
                session_id: None,
            })
            .await
            .unwrap();
        assert!(!result.ok);
        assert!(result.message.contains("Unknown engine"));
    }

    #[tokio::test]
    async fn test_postgres_missing_db_structured_fail() {
        let mgr = ConnectionManager::bootstrap();
        let result = mgr
            .test_connection(&OpenSessionRequest {
                engine: "postgres".into(),
                plugin_config: json!({ "host": "127.0.0.1", "port": 5432 }),
                password: None,
                pool: Some(PoolConfig {
                    max_size: 1,
                    min_idle: 0,
                    acquire_timeout_ms: 1500,
                    idle_timeout_secs: 5,
                }),
                session_id: None,
            })
            .await
            .unwrap();
        assert!(!result.ok, "expected fail");
        println!("msg={}", result.message);
        assert!(
            result.message.contains("database") || result.message.contains("missing"),
            "msg={}",
            result.message
        );
    }

    #[tokio::test]
    async fn test_postgres_unreachable_structured_fail() {
        let mgr = ConnectionManager::bootstrap();
        let result = mgr
            .test_connection(&OpenSessionRequest {
                engine: "postgres".into(),
                plugin_config: json!({
                    "host": "127.0.0.1",
                    "port": 1,
                    "database": "postgres",
                    "username": "postgres"
                }),
                password: Some("x".into()),
                pool: Some(PoolConfig {
                    max_size: 1,
                    min_idle: 0,
                    acquire_timeout_ms: 1500,
                    idle_timeout_secs: 5,
                }),
                session_id: None,
            })
            .await
            .unwrap();
        assert!(!result.ok);
        println!("unreachable msg={}", result.message);
        assert!(!result.message.is_empty());
        // Prefer root-cause wording over opaque wrappers
        assert!(
            result.message.to_lowercase().contains("refused")
                || result.message.to_lowercase().contains("connect")
                || result.message.to_lowercase().contains("error"),
            "msg={}",
            result.message
        );
    }

    #[tokio::test]
    async fn test_postgres_local_success_when_available() {
        let user = std::env::var("USER").unwrap_or_else(|_| "postgres".into());
        let mgr = ConnectionManager::bootstrap();
        let result = mgr
            .test_connection(&OpenSessionRequest {
                engine: "postgres".into(),
                plugin_config: json!({
                    "host": "127.0.0.1",
                    "port": 5432,
                    "database": "postgres",
                    "username": user,
                }),
                password: None,
                pool: Some(PoolConfig {
                    max_size: 1,
                    min_idle: 0,
                    acquire_timeout_ms: 3000,
                    idle_timeout_secs: 5,
                }),
                session_id: None,
            })
            .await
            .unwrap();
        println!(
            "local success? ok={} msg={} version={:?}",
            result.ok, result.message, result.server_version
        );
        // Skip-friendly: if Postgres isn't running, just assert structured fail.
        if result.ok {
            assert!(result
                .server_version
                .as_deref()
                .unwrap_or("")
                .contains("PostgreSQL"));
        } else {
            assert!(!result.message.is_empty());
        }
    }
}
