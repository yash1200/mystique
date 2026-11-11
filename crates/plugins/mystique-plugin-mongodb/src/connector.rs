use std::time::Instant;

use async_trait::async_trait;
use mongodb::bson::doc;
use mystique_connector_api::{
    tls_form_fields, ConnectionConfig, ConnectionFormSchema, ConnectorError, ConnectorResult,
    ConnectorSession, DatabaseConnector, EngineCapabilities, EngineFamily, FormField,
    FormFieldType, FormattedConnectionUrl, TestResult,
};

use crate::session::MongoSession;

mod client;
pub(crate) mod url;

use client::build_client;
use url::format_mongo_url;

pub struct MongoConnector;

#[async_trait]
impl DatabaseConnector for MongoConnector {
    fn id(&self) -> &'static str {
        "mongodb"
    }

    fn display_name(&self) -> &'static str {
        "MongoDB"
    }

    fn family(&self) -> EngineFamily {
        EngineFamily::Document
    }

    fn description(&self) -> &'static str {
        "Document store with driver-managed connection pooling"
    }

    fn capabilities(&self) -> EngineCapabilities {
        crate::default_capabilities()
    }

    fn connection_form_schema(&self) -> ConnectionFormSchema {
        ConnectionFormSchema {
            engine: "mongodb".into(),
            fields: vec![FormField {
                name: "uri".into(),
                label: "Connection URI".into(),
                field_type: FormFieldType::Uri,
                required: true,
                secret: true,
                default: None,
                placeholder: Some(
                    "mongodb://user:pass@127.0.0.1:27017/mydb?authSource=admin".into(),
                ),
                hint: None,
            }]
            .into_iter()
            .chain(tls_form_fields())
            .collect(),
        }
    }

    fn format_connection_url(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<FormattedConnectionUrl> {
        format_mongo_url(config)
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> ConnectorResult<TestResult> {
        let start = Instant::now();
        let (client, default_db) = build_client(config).await?;
        client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(|e| ConnectorError::Connect(e.to_string()))?;

        let version = client
            .database("admin")
            .run_command(doc! { "buildInfo": 1 })
            .await
            .ok()
            .and_then(|d| d.get_str("version").ok().map(|s| s.to_string()));

        let message = match default_db.as_deref() {
            Some(db) => format!("Connected (default db: {db})"),
            None => "Connected (no default database in URI — pick one in the explorer)".into(),
        };

        Ok(TestResult {
            ok: true,
            engine: "mongodb".into(),
            server_version: version,
            latency_ms: start.elapsed().as_millis() as u64,
            message,
            ssh_host_key: None,
        })
    }

    async fn connect(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<Box<dyn ConnectorSession>> {
        let (client, default_db) = build_client(config).await?;
        client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(|e| ConnectorError::Connect(e.to_string()))?;

        let session = MongoSession::new(client, config.pool.max_size, default_db);
        session.note_transactions().await;
        Ok(Box::new(session))
    }
}
