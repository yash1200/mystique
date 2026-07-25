use std::error::Error as StdError;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use deadpool_postgres::{Config as PoolCfg, ManagerConfig, RecyclingMethod, Runtime, SslMode};
use mystique_connector_api::{
    ConnectionConfig, ConnectionFormSchema, ConnectorError, ConnectorResult, ConnectorSession,
    DatabaseConnector, EngineCapabilities, EngineFamily, FormField, FormFieldType, TestResult,
};
use tokio_postgres::NoTls;

use crate::session::PostgresSession;

pub struct PostgresConnector;

/// Flatten nested error sources so users see the real Postgres FATAL, not deadpool's "db error".
fn format_error(err: &dyn StdError) -> String {
    let mut parts = Vec::new();
    let mut cur: Option<&dyn StdError> = Some(err);
    while let Some(e) = cur {
        let s = e.to_string();
        if parts.last().map(|p: &String| p != &s).unwrap_or(true) {
            parts.push(s);
        }
        cur = e.source();
    }
    if parts.is_empty() {
        "unknown error".into()
    } else {
        parts.join(" — ")
    }
}

fn wants_ssl(config: &ConnectionConfig) -> bool {
    if config.bool_field("ssl", false) {
        return true;
    }
    match config
        .str_field("sslMode")
        .or_else(|| config.str_field("ssl_mode"))
        .as_deref()
    {
        Some("require") | Some("verify-ca") | Some("verify-full") | Some("prefer") => true,
        _ => false,
    }
}

fn build_pool_config(config: &ConnectionConfig) -> ConnectorResult<PoolCfg> {
    let host = config
        .str_field("host")
        .unwrap_or_else(|| "127.0.0.1".into());
    let port = config.u16_field("port", 5432);
    let dbname = config.require_str("database")?;
    let user = config
        .str_field("username")
        .or_else(|| config.str_field("user"))
        .unwrap_or_else(|| "postgres".into());

    let mut cfg = PoolCfg::new();
    cfg.host = Some(host);
    cfg.port = Some(port);
    cfg.dbname = Some(dbname);
    cfg.user = Some(user);
    if let Some(pw) = config.password.clone() {
        cfg.password = Some(pw);
    } else if let Some(pw) = config.str_field("password") {
        cfg.password = Some(pw);
    }

    // Fail fast instead of hanging on unreachable hosts.
    let timeout_ms = config.pool.acquire_timeout_ms.max(1_000);
    cfg.connect_timeout = Some(Duration::from_millis(timeout_ms.min(30_000)));

    if wants_ssl(config) {
        // Advertise SSL to the server; TLS connector is chosen in create_pool / connect_once.
        cfg.ssl_mode = Some(SslMode::Require);
    } else {
        cfg.ssl_mode = Some(SslMode::Disable);
    }

    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    let mut pool = deadpool_postgres::PoolConfig::new(config.pool.max_size.max(1) as usize);
    pool.timeouts.wait = Some(Duration::from_millis(timeout_ms));
    pool.timeouts.create = Some(Duration::from_millis(timeout_ms));
    if config.pool.idle_timeout_secs > 0 {
        pool.timeouts.recycle = Some(Duration::from_secs(config.pool.idle_timeout_secs));
    }
    cfg.pool = Some(pool);
    Ok(cfg)
}

fn build_pg_config(config: &ConnectionConfig) -> ConnectorResult<tokio_postgres::Config> {
    let host = config
        .str_field("host")
        .unwrap_or_else(|| "127.0.0.1".into());
    let port = config.u16_field("port", 5432);
    let dbname = config.require_str("database")?;
    let user = config
        .str_field("username")
        .or_else(|| config.str_field("user"))
        .unwrap_or_else(|| "postgres".into());

    let mut pg = tokio_postgres::Config::new();
    pg.host(&host);
    pg.port(port);
    pg.dbname(&dbname);
    pg.user(&user);
    if let Some(pw) = config.password.as_deref() {
        pg.password(pw);
    } else if let Some(pw) = config.str_field("password") {
        pg.password(pw);
    }

    let timeout_ms = config.pool.acquire_timeout_ms.max(1_000);
    pg.connect_timeout(Duration::from_millis(timeout_ms.min(30_000)));
    pg.application_name("MystiqueDB");

    if wants_ssl(config) {
        pg.ssl_mode(tokio_postgres::config::SslMode::Require);
    } else {
        pg.ssl_mode(tokio_postgres::config::SslMode::Disable);
    }

    Ok(pg)
}

/// One-shot connect used by test_connection (clear errors, no pool wrapper).
async fn connect_once(
    config: &ConnectionConfig,
) -> ConnectorResult<(tokio_postgres::Client, tokio::task::JoinHandle<()>)> {
    let pg = build_pg_config(config)?;

    if wants_ssl(config) {
        let tls_connector = native_tls::TlsConnector::builder()
            .build()
            .map_err(|e| ConnectorError::Connect(format!("TLS init failed: {e}")))?;
        let tls = postgres_native_tls::MakeTlsConnector::new(tls_connector);
        let (client, connection) = pg
            .connect(tls)
            .await
            .map_err(|e| ConnectorError::Connect(format_error(&e)))?;
        let handle = tokio::spawn(async move {
            let _ = connection.await;
        });
        Ok((client, handle))
    } else {
        let (client, connection) = pg
            .connect(NoTls)
            .await
            .map_err(|e| ConnectorError::Connect(format_error(&e)))?;
        let handle = tokio::spawn(async move {
            let _ = connection.await;
        });
        Ok((client, handle))
    }
}

async fn create_pool(config: &ConnectionConfig) -> ConnectorResult<deadpool_postgres::Pool> {
    let pool_cfg = build_pool_config(config)?;
    if wants_ssl(config) {
        let tls_connector = native_tls::TlsConnector::builder()
            .build()
            .map_err(|e| ConnectorError::Connect(format!("TLS init failed: {e}")))?;
        let tls = postgres_native_tls::MakeTlsConnector::new(tls_connector);
        pool_cfg
            .create_pool(Some(Runtime::Tokio1), tls)
            .map_err(|e| ConnectorError::Pool(format_error(&e)))
    } else {
        pool_cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| ConnectorError::Pool(format_error(&e)))
    }
}

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
        EngineCapabilities::postgres_defaults()
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
                    hint: Some("Homebrew/local often uses your OS username, not `postgres`.".into()),
                },
                FormField {
                    name: "username".into(),
                    label: "Username".into(),
                    field_type: FormFieldType::Text,
                    required: true,
                    secret: false,
                    default: Some(serde_json::json!("postgres")),
                    placeholder: None,
                    hint: Some("Docker images use `postgres`; Homebrew uses your macOS user.".into()),
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
        let (client, conn_task) = connect_once(config).await?;

        let row = client
            .query_one("SELECT version()", &[])
            .await
            .map_err(|e| ConnectorError::Query(format_error(&e)))?;
        let version: String = row.get(0);
        let latency_ms = start.elapsed().as_millis() as u64;

        // Drop client first so the connection task can finish cleanly.
        drop(client);
        let _ = conn_task.await;

        Ok(TestResult {
            ok: true,
            engine: "postgres".into(),
            server_version: Some(version),
            latency_ms,
            message: "Connected".into(),
        })
    }

    async fn connect(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<Box<dyn ConnectorSession>> {
        let pool = create_pool(config).await?;

        // Warm / validate: acquire one connection
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
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::PoolConfig;
    use serde_json::json;

    #[tokio::test]
    async fn test_connection_refused_returns_err() {
        let config = ConnectionConfig {
            engine: "postgres".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 1,
                "database": "postgres",
                "username": "postgres"
            }),
            password: Some("postgres".into()),
            pool: PoolConfig {
                max_size: 1,
                min_idle: 0,
                acquire_timeout_ms: 2000,
                idle_timeout_secs: 10,
            },
        };
        let connector = PostgresConnector;
        let err = connector
            .test_connection(&config)
            .await
            .expect_err("port 1 should refuse");
        let s = err.to_string();
        println!("refused: {s}");
        assert!(
            s.contains("error")
                || s.contains("connect")
                || s.contains("refused")
                || s.contains("Connection"),
            "unexpected error shape: {s}"
        );
    }

    #[test]
    fn build_pool_config_reads_fields() {
        let config = ConnectionConfig {
            engine: "postgres".into(),
            plugin_config: json!({
                "host": "db.example.com",
                "port": 5433,
                "database": "app",
                "username": "appuser",
                "password": "inline-secret"
            }),
            password: Some("override-secret".into()),
            pool: PoolConfig::default(),
        };
        let cfg = build_pool_config(&config).expect("config");
        assert_eq!(cfg.host.as_deref(), Some("db.example.com"));
        assert_eq!(cfg.port, Some(5433));
        assert_eq!(cfg.dbname.as_deref(), Some("app"));
        assert_eq!(cfg.user.as_deref(), Some("appuser"));
        assert_eq!(cfg.password.as_deref(), Some("override-secret"));
        assert!(cfg.connect_timeout.is_some());
    }

    #[test]
    fn build_pool_config_requires_database() {
        let config = ConnectionConfig {
            engine: "postgres".into(),
            plugin_config: json!({ "host": "127.0.0.1" }),
            password: None,
            pool: PoolConfig::default(),
        };
        let err = build_pool_config(&config).unwrap_err();
        assert!(err.to_string().contains("database"), "{err}");
    }

    #[tokio::test]
    async fn test_connection_against_local_homebrew() {
        let user = std::env::var("USER").unwrap_or_else(|_| "yashjohri".into());
        let config = ConnectionConfig {
            engine: "postgres".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 5432,
                "database": "postgres",
                "username": user
            }),
            password: None,
            pool: PoolConfig {
                max_size: 1,
                min_idle: 0,
                acquire_timeout_ms: 5000,
                idle_timeout_secs: 10,
            },
        };
        let connector = PostgresConnector;
        let result = connector.test_connection(&config).await.expect("connect");
        assert!(result.ok, "msg={}", result.message);
        assert!(
            result
                .server_version
                .as_deref()
                .unwrap_or("")
                .contains("PostgreSQL"),
            "version={:?}",
            result.server_version
        );
        println!(
            "homebrew ok latency={} version={:?}",
            result.latency_ms, result.server_version
        );
    }

    #[tokio::test]
    async fn test_connection_default_user_postgres_has_clear_error() {
        let config = ConnectionConfig {
            engine: "postgres".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 5432,
                "database": "postgres"
                // no username → backend defaults to "postgres"
            }),
            password: None,
            pool: PoolConfig {
                max_size: 1,
                min_idle: 0,
                acquire_timeout_ms: 5000,
                idle_timeout_secs: 10,
            },
        };
        let connector = PostgresConnector;
        let err = connector
            .test_connection(&config)
            .await
            .expect_err("role postgres should not exist on homebrew");
        let s = err.to_string();
        println!("default user err: {s}");
        // Must surface the real FATAL, not deadpool's opaque "db error"
        assert!(
            s.to_lowercase().contains("postgres")
                && (s.to_lowercase().contains("role")
                    || s.to_lowercase().contains("exist")
                    || s.to_lowercase().contains("authentication")
                    || s.to_lowercase().contains("password")),
            "error should mention the real postgres failure, got: {s}"
        );
    }
}
