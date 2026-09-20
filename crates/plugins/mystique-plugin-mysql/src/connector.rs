use std::time::Instant;

use async_trait::async_trait;
use mysql_async::prelude::*;
use mysql_async::Pool;
use mystique_connector_api::{
    ConnectionConfig, ConnectionFormSchema, ConnectorError, ConnectorResult, ConnectorSession,
    DatabaseConnector, EngineCapabilities, EngineFamily, FormField, FormFieldType,
    FormattedConnectionUrl, ParsedConnectionUrl, TestResult,
};

use crate::session::MysqlSession;

pub(crate) mod pool;
pub(crate) mod url;

use pool::{build_opts, connect_once, format_error};
use url::{format_mysql_url, parse_mysql_url};

pub struct MysqlConnector;

#[async_trait]
impl DatabaseConnector for MysqlConnector {
    fn id(&self) -> &'static str {
        "mysql"
    }

    fn display_name(&self) -> &'static str {
        "MySQL"
    }

    fn family(&self) -> EngineFamily {
        EngineFamily::Relational
    }

    fn description(&self) -> &'static str {
        "MySQL / MariaDB relational studio with connection pooling (mysql_async)"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::mysql_defaults()
    }

    fn connection_form_schema(&self) -> ConnectionFormSchema {
        ConnectionFormSchema {
            engine: "mysql".into(),
            fields: vec![
                FormField {
                    name: "host".into(),
                    label: "Host".into(),
                    field_type: FormFieldType::Text,
                    required: true,
                    secret: false,
                    default: Some(serde_json::json!("127.0.0.1")),
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "port".into(),
                    label: "Port".into(),
                    field_type: FormFieldType::Number,
                    required: true,
                    secret: false,
                    default: Some(serde_json::json!(3306)),
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "database".into(),
                    label: "Database".into(),
                    field_type: FormFieldType::Text,
                    required: true,
                    secret: false,
                    default: Some(serde_json::json!("mystique")),
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "username".into(),
                    label: "Username".into(),
                    field_type: FormFieldType::Text,
                    required: true,
                    secret: false,
                    default: Some(serde_json::json!("admin")),
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "password".into(),
                    label: "Password".into(),
                    field_type: FormFieldType::Password,
                    required: false,
                    secret: true,
                    default: None,
                    placeholder: None,
                    hint: None,
                },
            ],
        }
    }

    fn parse_connection_url(&self, url: &str) -> ConnectorResult<ParsedConnectionUrl> {
        parse_mysql_url(url)
    }

    fn format_connection_url(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<FormattedConnectionUrl> {
        format_mysql_url(config)
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> ConnectorResult<TestResult> {
        let start = Instant::now();
        let (mut conn, pool) = connect_once(config).await?;

        let version: Option<String> = conn
            .query_first("SELECT VERSION()")
            .await
            .map_err(|e| ConnectorError::Query(format_error(&e)))?;
        let latency_ms = start.elapsed().as_millis() as u64;

        drop(conn);
        pool.disconnect()
            .await
            .map_err(|e| ConnectorError::Connect(format_error(&e)))?;

        Ok(TestResult {
            ok: true,
            engine: "mysql".into(),
            server_version: version,
            latency_ms,
            message: "Connected".into(),
            ssh_host_key: None,
        })
    }

    async fn connect(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<Box<dyn ConnectorSession>> {
        let opts = build_opts(config)?;
        let pool = Pool::new(opts);

        {
            let mut conn = pool
                .get_conn()
                .await
                .map_err(|e| ConnectorError::Connect(format_error(&e)))?;
            conn.query_drop("SELECT 1")
                .await
                .map_err(|e| ConnectorError::Connect(format_error(&e)))?;
        }

        Ok(Box::new(MysqlSession::new(pool, config.pool.max_size)))
    }
}
