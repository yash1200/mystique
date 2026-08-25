use std::time::{Duration, Instant};

use async_trait::async_trait;
use mongodb::bson::doc;
use mongodb::options::{ClientOptions, Credential, ServerAddress};
use mongodb::Client;
use mystique_connector_api::{
    ConnectionConfig, ConnectionFormSchema, ConnectorError, ConnectorResult, ConnectorSession,
    DatabaseConnector, EngineCapabilities, EngineFamily, FormField, FormFieldType,
    FormattedConnectionUrl, TestResult,
};

use crate::session::MongoSession;

pub struct MongoConnector;

/// Build a Mongo client and resolve the default application database.
///
/// Default DB resolution order:
/// 1. explicit `pluginConfig.database`
/// 2. path segment in the connection URI (`…/mystique?…`)
/// 3. `None` (caller / explorer must pick a database)
async fn build_client(config: &ConnectionConfig) -> ConnectorResult<(Client, Option<String>)> {
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

    options.app_name = Some("Mystique".into());

    let default_db = config
        .str_field("database")
        .filter(|s| !s.is_empty())
        .or_else(|| options.default_database.clone())
        .or_else(|| {
            config
                .str_field("uri")
                .and_then(|u| database_from_mongo_uri(&u))
        });

    let client =
        Client::with_options(options).map_err(|e| ConnectorError::Connect(e.to_string()))?;
    Ok((client, default_db))
}

/// Extract `/dbname` from a mongodb:// or mongodb+srv:// URI (before `?` / `#`).
fn database_from_mongo_uri(uri: &str) -> Option<String> {
    let rest = uri
        .strip_prefix("mongodb://")
        .or_else(|| uri.strip_prefix("mongodb+srv://"))?;
    // Authority ends at first `/` that starts the path (not credentials@host).
    let after_at = rest
        .rsplit_once('@')
        .map(|(_, host_path)| host_path)
        .unwrap_or(rest);
    let path = after_at.split_once('/')?.1;
    let name = path.split(['?', '#']).next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn encode_url_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Prefer stored URI; otherwise assemble `mongodb://` from discrete fields.
fn format_mongo_url(config: &ConnectionConfig) -> ConnectorResult<FormattedConnectionUrl> {
    if let Some(uri) = config.str_field("uri") {
        let uri = uri.trim().to_string();
        if uri.is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "connection URI is empty".into(),
            ));
        }
        return Ok(FormattedConnectionUrl {
            engine: "mongodb".into(),
            url: uri,
        });
    }

    let host = config
        .str_field("host")
        .unwrap_or_else(|| "127.0.0.1".into());
    let port = config.u16_field("port", 27017);
    let database = config.str_field("database").unwrap_or_default();
    let user = config
        .str_field("username")
        .or_else(|| config.str_field("user"))
        .unwrap_or_default();
    let password = config
        .password
        .clone()
        .or_else(|| config.str_field("password"))
        .filter(|s| !s.is_empty());
    let auth_source = config
        .str_field("authSource")
        .or_else(|| config.str_field("auth_source"))
        .unwrap_or_else(|| {
            if user.is_empty() {
                String::new()
            } else {
                "admin".into()
            }
        });

    let mut authority = format!("{host}:{port}");
    if !user.is_empty() {
        let enc_user = encode_url_component(&user);
        let cred = match password.as_deref() {
            Some(pw) => format!("{enc_user}:{}", encode_url_component(pw)),
            None => enc_user,
        };
        authority = format!("{cred}@{authority}");
    }
    let path = if database.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", encode_url_component(&database))
    };
    let qs = if auth_source.is_empty() {
        String::new()
    } else {
        format!("?authSource={}", encode_url_component(&auth_source))
    };

    Ok(FormattedConnectionUrl {
        engine: "mongodb".into(),
        url: format!("mongodb://{authority}{path}{qs}"),
    })
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
            }],
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
                "uri": "mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin",
                "database": "mystique",
            }),
            password: None,
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
                println!(
                    "ok latency={}ms version={:?}",
                    r.latency_ms, r.server_version
                );
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
