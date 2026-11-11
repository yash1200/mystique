use std::time::Instant;

use async_trait::async_trait;
use mystique_connector_api::{
    tls_form_fields, ConnectionConfig, ConnectionFormSchema, ConnectorError, ConnectorResult,
    ConnectorSession, DatabaseConnector, EngineCapabilities, EngineFamily, FormField,
    FormFieldType, FormattedConnectionUrl, ParsedConnectionUrl, TestResult,
};

use crate::session::PostgresSession;

pub(crate) mod notices;
pub(crate) mod pool;
pub(crate) mod url;

use pool::{connect_once, create_pool, format_error, pg_col};
use url::{format_postgres_url, parse_postgres_url};

pub struct PostgresConnector;

#[async_trait]
impl DatabaseConnector for PostgresConnector {
    fn id(&self) -> &'static str {
        "postgres"
    }

    fn display_name(&self) -> &'static str {
        "PostgreSQL"
    }

    fn family(&self) -> EngineFamily {
        EngineFamily::Relational
    }

    fn description(&self) -> &'static str {
        "Relational studio with connection pooling (deadpool + tokio-postgres)"
    }

    fn capabilities(&self) -> EngineCapabilities {
        crate::default_capabilities()
    }

    fn connection_form_schema(&self) -> ConnectionFormSchema {
        ConnectionFormSchema {
            engine: "postgres".into(),
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
                    default: Some(serde_json::json!(5432)),
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "database".into(),
                    label: "Database".into(),
                    field_type: FormFieldType::Text,
                    required: true,
                    secret: false,
                    default: Some(serde_json::json!("postgres")),
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "username".into(),
                    label: "Username".into(),
                    field_type: FormFieldType::Text,
                    required: true,
                    secret: false,
                    default: Some(serde_json::json!("postgres")),
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
            ]
            .into_iter()
            .chain(tls_form_fields())
            .collect(),
        }
    }

    fn parse_connection_url(&self, url: &str) -> ConnectorResult<ParsedConnectionUrl> {
        parse_postgres_url(url)
    }

    fn format_connection_url(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<FormattedConnectionUrl> {
        format_postgres_url(config)
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> ConnectorResult<TestResult> {
        let start = Instant::now();
        let (client, conn_task) = connect_once(config).await?;

        let row = client
            .query_one("SELECT version()", &[])
            .await
            .map_err(|e| ConnectorError::Query(format_error(&e)))?;
        let version: String = pg_col(&row, 0)?;
        let latency_ms = start.elapsed().as_millis() as u64;

        drop(client);
        let _ = conn_task.await;

        Ok(TestResult {
            ok: true,
            engine: "postgres".into(),
            server_version: Some(version),
            latency_ms,
            message: "Connected".into(),
            ssh_host_key: None,
        })
    }

    async fn connect(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<Box<dyn ConnectorSession>> {
        let (pool, cancel_tls, notices) = create_pool(config).await?;

        {
            let client = pool
                .get()
                .await
                .map_err(|e| ConnectorError::Connect(format_error(&e)))?;
            client
                .simple_query("SELECT 1")
                .await
                .map_err(|e| ConnectorError::Connect(format_error(&e)))?;
        }

        Ok(Box::new(PostgresSession::new(
            pool,
            config.pool.max_size,
            cancel_tls,
            notices,
        )))
    }
}
