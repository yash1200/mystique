use std::time::{Duration, Instant};

use async_trait::async_trait;
use mongodb::bson::doc;
use mongodb::options::{ClientOptions, Credential, ServerAddress};
use mongodb::Client;
use mystique_connector_api::{
    ConnectionConfig, ConnectionFormSchema, ConnectorError, ConnectorResult, ConnectorSession,
    DatabaseConnector, EngineCapabilities, EngineFamily, FormField, FormFieldType, TestResult,
};

use crate::session::MongoSession;

pub struct MongoConnector;

async fn build_client(config: &ConnectionConfig) -> ConnectorResult<Client> {
    let mut options = if let Some(uri) = config.str_field("uri") {
        ClientOptions::parse(&uri)
            .await
            .map_err(|e| ConnectorError::InvalidConfig(e.to_string()))?
    } else {
        let host = config
            .str_field("host")
            .unwrap_or_else(|| "127.0.0.1".into());
        let port = config.u16_field("port", 27017);
        let mut opts = ClientOptions::default();
        opts.hosts = vec![ServerAddress::Tcp {
            host,
            port: Some(port),
        }];
        opts
    };

    // Pool sizing (driver-managed)
    options.max_pool_size = Some(config.pool.max_size);
    options.min_pool_size = Some(config.pool.min_idle.min(config.pool.max_size));
    options.connect_timeout = Some(Duration::from_millis(config.pool.acquire_timeout_ms));
    if config.pool.idle_timeout_secs > 0 {
        options.max_idle_time = Some(Duration::from_secs(config.pool.idle_timeout_secs));
    }

    // Auth from fields if not already embedded in URI
    let user = config.str_field("username");
    let password = config
        .password
        .clone()
        .or_else(|| config.str_field("password"));
    if let (Some(username), Some(password)) = (user, password) {
        let mut cred = Credential::default();
        cred.username = Some(username);
        cred.password = Some(password);
        // Prefer explicit authSource; default to admin (root-style users), not the app DB name.
        cred.source = Some(
            config
                .str_field("authSource")
                .or_else(|| config.str_field("auth_source"))
                .unwrap_or_else(|| "admin".into()),
        );
        options.credential = Some(cred);
    }

    options.app_name = Some("MystiqueDB".into());

    Client::with_options(options).map_err(|e| ConnectorError::Connect(e.to_string()))
}

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
        EngineCapabilities::mongodb_defaults()
    }

    fn connection_form_schema(&self) -> ConnectionFormSchema {
        ConnectionFormSchema {
            engine: "mongodb".into(),
            fields: vec![
                FormField {
                    name: "uri".into(),
                    label: "Connection URI".into(),
                    field_type: FormFieldType::Uri,
                    required: false,
                    secret: true,
                    default: None,
                    placeholder: Some("mongodb://127.0.0.1:27017".into()),
                    hint: Some("If set, preferred over host fields".into()),
                },
                FormField {
                    name: "host".into(),
                    label: "Host".into(),
                    field_type: FormFieldType::Text,
                    required: false,
                    secret: false,
                    default: Some(serde_json::json!("127.0.0.1")),
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "port".into(),
                    label: "Port".into(),
                    field_type: FormFieldType::Number,
                    required: false,
                    secret: false,
                    default: Some(serde_json::json!(27017)),
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "database".into(),
                    label: "Database".into(),
                    field_type: FormFieldType::Text,
                    required: false,
                    secret: false,
                    default: None,
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "username".into(),
                    label: "Username".into(),
                    field_type: FormFieldType::Text,
                    required: false,
                    secret: false,
                    default: None,
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

    async fn test_connection(&self, config: &ConnectionConfig) -> ConnectorResult<TestResult> {
        let start = Instant::now();
        let client = build_client(config).await?;
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

        Ok(TestResult {
            ok: true,
            engine: "mongodb".into(),
            server_version: version,
            latency_ms: start.elapsed().as_millis() as u64,
            message: "Connected".into(),
        })
    }

    async fn connect(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<Box<dyn ConnectorSession>> {
        let client = build_client(config).await?;
        client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(|e| ConnectorError::Connect(e.to_string()))?;

        let default_db = config.str_field("database");
        Ok(Box::new(MongoSession::new(
            client,
            config.pool.max_size,
            default_db,
        )))
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use mystique_connector_api::PoolConfig;
    use serde_json::json;

    #[tokio::test]
    async fn test_connection_against_local_docker() {
        // Skip if mongo isn't up
        let config = ConnectionConfig {
            engine: "mongodb".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 27017,
                "database": "mystique_dev",
                "username": "abc",
                "authSource": "admin"
            }),
            password: Some("abc".into()),
            pool: PoolConfig {
                max_size: 2,
                min_idle: 0,
                acquire_timeout_ms: 5000,
                idle_timeout_secs: 30,
            },
        };
        let connector = MongoConnector;
        match connector.test_connection(&config).await {
            Ok(r) => {
                assert!(r.ok, "expected ok: {}", r.message);
                println!("ok latency={}ms version={:?}", r.latency_ms, r.server_version);
            }
            Err(e) => {
                // Environment without docker mongo
                eprintln!("skip/fail: {e}");
                // Fail hard only if connection refused isn't the issue - always assert for CI local
                panic!("mongo test_connection failed: {e}");
            }
        }
    }
}
