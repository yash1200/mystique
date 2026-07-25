use std::path::PathBuf;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use mystique_connector_api::{
    ConnectionConfig, ConnectionFormSchema, ConnectorError, ConnectorResult, ConnectorSession,
    DatabaseConnector, EngineCapabilities, EngineFamily, FormField, FormFieldType, TestResult,
};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OpenFlags;

use crate::session::SqliteSession;

pub struct SqliteConnector;

fn resolve_path(config: &ConnectionConfig) -> ConnectorResult<PathBuf> {
    let path = config
        .str_field("path")
        .or_else(|| config.str_field("database"))
        .ok_or_else(|| ConnectorError::InvalidConfig("missing `path`".into()))?;
    Ok(PathBuf::from(path))
}

fn build_pool(config: &ConnectionConfig) -> ConnectorResult<Pool<SqliteConnectionManager>> {
    let path = resolve_path(config)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(ConnectorError::InvalidConfig(format!(
                "parent directory does not exist: {}",
                parent.display()
            )));
        }
    }

    let flags = if config.bool_field("readOnly", false) || config.bool_field("read_only", false) {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };

    let manager = SqliteConnectionManager::file(&path).with_flags(flags);
    let pool = Pool::builder()
        .max_size(config.pool.max_size.max(1))
        .min_idle(Some(config.pool.min_idle.min(config.pool.max_size).max(0)))
        .connection_timeout(Duration::from_millis(config.pool.acquire_timeout_ms))
        .build(manager)
        .map_err(|e| ConnectorError::Pool(e.to_string()))?;
    Ok(pool)
}

#[async_trait]
impl DatabaseConnector for SqliteConnector {
    fn id(&self) -> &'static str {
        "sqlite"
    }

    fn display_name(&self) -> &'static str {
        "SQLite"
    }

    fn family(&self) -> EngineFamily {
        EngineFamily::Relational
    }

    fn description(&self) -> &'static str {
        "Local SQL file with r2d2 connection pooling"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::sqlite_defaults()
    }

    fn connection_form_schema(&self) -> ConnectionFormSchema {
        ConnectionFormSchema {
            engine: "sqlite".into(),
            fields: vec![FormField {
                name: "path".into(),
                label: "File path".into(),
                field_type: FormFieldType::Path,
                required: true,
                secret: false,
                default: None,
                placeholder: Some("/path/to/database.sqlite".into()),
                hint: Some("Absolute or relative path to the .db / .sqlite file".into()),
            }],
        }
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> ConnectorResult<TestResult> {
        let start = Instant::now();
        let path = resolve_path(config)?;
        let config = config.clone();
        let result = tokio::task::spawn_blocking(move || {
            let pool = build_pool(&config)?;
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Connect(e.to_string()))?;
            let version: String = conn
                .query_row("SELECT sqlite_version()", [], |r| r.get(0))
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            Ok::<_, ConnectorError>(version)
        })
        .await
        .map_err(|e| ConnectorError::Internal(e.to_string()))??;

        Ok(TestResult {
            ok: true,
            engine: "sqlite".into(),
            server_version: Some(format!("SQLite {result}")),
            latency_ms: start.elapsed().as_millis() as u64,
            message: format!("Opened {}", path.display()),
        })
    }

    async fn connect(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<Box<dyn ConnectorSession>> {
        let config = config.clone();
        let max_size = config.pool.max_size;
        let pool = tokio::task::spawn_blocking(move || build_pool(&config))
            .await
            .map_err(|e| ConnectorError::Internal(e.to_string()))??;

        // validate
        {
            let pool2 = pool.clone();
            tokio::task::spawn_blocking(move || {
                let conn = pool2
                    .get()
                    .map_err(|e| ConnectorError::Connect(e.to_string()))?;
                conn.query_row("SELECT 1", [], |_| Ok(()))
                    .map_err(|e| ConnectorError::Connect(e.to_string()))
            })
            .await
            .map_err(|e| ConnectorError::Internal(e.to_string()))??;
        }

        Ok(Box::new(SqliteSession::new(pool, max_size)))
    }
}
