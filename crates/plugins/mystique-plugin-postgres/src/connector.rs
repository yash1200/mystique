use std::error::Error as StdError;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use deadpool_postgres::{Config as PoolCfg, ManagerConfig, RecyclingMethod, Runtime, SslMode};
use mystique_connector_api::{
    ConnectionConfig, ConnectionFormSchema, ConnectorError, ConnectorResult, ConnectorSession,
    DatabaseConnector, EngineCapabilities, EngineFamily, FormField, FormFieldType,
    FormattedConnectionUrl, ParsedConnectionUrl, TestResult,
};
use tokio_postgres::config::{Host, SslMode as PgSslMode};
use tokio_postgres::NoTls;

use crate::session::PostgresSession;

pub struct PostgresConnector;

/// Parse `postgres://` / `postgresql://` (or libpq key=value) into form fields.
fn parse_postgres_url(url: &str) -> ConnectorResult<ParsedConnectionUrl> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "connection URL is empty".into(),
        ));
    }

    let cfg: tokio_postgres::Config = trimmed.parse().map_err(|e| {
        ConnectorError::InvalidConfig(format!("invalid PostgreSQL connection URL: {e}"))
    })?;

    let host = cfg
        .get_hosts()
        .iter()
        .find_map(|h| match h {
            Host::Tcp(name) => Some(name.clone()),
            #[cfg(unix)]
            Host::Unix(_) => None,
        })
        .or_else(|| {
            cfg.get_hostaddrs()
                .first()
                .map(|ip| ip.to_string())
        })
        .unwrap_or_else(|| "127.0.0.1".into());

    let port = cfg.get_ports().first().copied().unwrap_or(5432);
    let database = cfg.get_dbname().unwrap_or("postgres").to_string();
    let username = cfg
        .get_user()
        .unwrap_or("postgres")
        .to_string();

    let password = cfg.get_password().map(|bytes| {
        String::from_utf8_lossy(bytes).into_owned()
    });

    // Form has a binary “Require SSL”. Only `sslmode=require` (and stricter)
    // turns it on — driver default `prefer` stays off so local URLs stay simple.
    let ssl = matches!(cfg.get_ssl_mode(), PgSslMode::Require);

    Ok(ParsedConnectionUrl {
        engine: "postgres".into(),
        plugin_config: serde_json::json!({
            "host": host,
            "port": port,
            "database": database,
            "username": username,
            "ssl": ssl,
            "sslMode": if ssl { "require" } else { "disable" },
        }),
        password,
    })
}

/// Format discrete plugin config into a `postgresql://` URL (clipboard / share).
fn format_postgres_url(config: &ConnectionConfig) -> ConnectorResult<FormattedConnectionUrl> {
    let host = config
        .str_field("host")
        .unwrap_or_else(|| "127.0.0.1".into());
    let port = config.u16_field("port", 5432);
    let database = config
        .str_field("database")
        .unwrap_or_else(|| "postgres".into());
    let user = config
        .str_field("username")
        .or_else(|| config.str_field("user"))
        .unwrap_or_else(|| "postgres".into());
    let password = config
        .password
        .clone()
        .or_else(|| config.str_field("password"))
        .filter(|s| !s.is_empty());
    let ssl = wants_ssl(config);

    // Keep host readable (IPv4 / hostnames); encode user/pass/db.
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
        String::new()
    } else {
        format!("/{}", encode_url_component(&database))
    };
    let qs = if ssl { "?sslmode=require" } else { "" };
    let url = format!("postgresql://{authority}{path}{qs}");

    // Round-trip through the driver parser so we never emit an unusable URL.
    let _ = url.parse::<tokio_postgres::Config>().map_err(|e| {
        ConnectorError::InvalidConfig(format!("failed to format PostgreSQL URL: {e}"))
    })?;

    Ok(FormattedConnectionUrl {
        engine: "postgres".into(),
        url,
    })
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
            ],
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

        Ok(Box::new(PostgresSession::new(pool, config.pool.max_size)))
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
    fn parse_connection_url_postgres_scheme() {
        let parsed = parse_postgres_url(
            "postgresql://appuser:s3cret@db.example.com:5433/appdb?sslmode=require",
        )
        .expect("parse");
        assert_eq!(parsed.engine, "postgres");
        assert_eq!(parsed.password.as_deref(), Some("s3cret"));
        let cfg = &parsed.plugin_config;
        assert_eq!(cfg["host"], "db.example.com");
        assert_eq!(cfg["port"], 5433);
        assert_eq!(cfg["database"], "appdb");
        assert_eq!(cfg["username"], "appuser");
        assert_eq!(cfg["ssl"], true);
        assert_eq!(cfg["sslMode"], "require");
    }

    #[test]
    fn parse_connection_url_defaults() {
        let parsed = parse_postgres_url("postgres://localhost/mydb").expect("parse");
        assert_eq!(parsed.plugin_config["host"], "localhost");
        assert_eq!(parsed.plugin_config["port"], 5432);
        assert_eq!(parsed.plugin_config["database"], "mydb");
        assert!(parsed.password.is_none());
    }

    #[test]
    fn parse_connection_url_rejects_empty() {
        let err = parse_postgres_url("   ").unwrap_err();
        assert!(err.to_string().to_lowercase().contains("empty"), "{err}");
    }

    #[test]
    fn format_connection_url_round_trip() {
        let config = ConnectionConfig {
            engine: "postgres".into(),
            plugin_config: json!({
                "host": "db.example.com",
                "port": 5433,
                "database": "appdb",
                "username": "appuser",
                "ssl": true,
                "sslMode": "require"
            }),
            password: Some("s3cret".into()),
            pool: PoolConfig::default(),
        };
        let formatted = format_postgres_url(&config).expect("format");
        assert!(
            formatted.url.starts_with("postgresql://"),
            "url={}",
            formatted.url
        );
        let parsed = parse_postgres_url(&formatted.url).expect("re-parse");
        assert_eq!(parsed.plugin_config["host"], "db.example.com");
        assert_eq!(parsed.plugin_config["port"], 5433);
        assert_eq!(parsed.plugin_config["database"], "appdb");
        assert_eq!(parsed.plugin_config["username"], "appuser");
        assert_eq!(parsed.password.as_deref(), Some("s3cret"));
        assert_eq!(parsed.plugin_config["ssl"], true);
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
