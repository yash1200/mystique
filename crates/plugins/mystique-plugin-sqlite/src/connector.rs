use std::time::Instant;

use async_trait::async_trait;
use mystique_connector_api::{
    ConnectionConfig, ConnectionFormSchema, ConnectorError, ConnectorResult, ConnectorSession,
    DatabaseConnector, EngineCapabilities, EngineFamily, FormField, FormFieldType,
    FormattedConnectionUrl, TestResult,
};

use crate::session::{query_failed, SqliteSession};

pub(crate) mod path;

use path::{build_pool, format_sqlite_url, resolve_path};

pub struct SqliteConnector;

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
                hint: None,
            }],
        }
    }

    fn format_connection_url(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<FormattedConnectionUrl> {
        format_sqlite_url(config)
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
                .map_err(query_failed)?;
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
            ssh_host_key: None,
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
