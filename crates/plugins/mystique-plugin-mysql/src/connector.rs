use std::time::Instant;

use async_trait::async_trait;
use mysql_async::prelude::*;
use mysql_async::{OptsBuilder, Pool, PoolConstraints, PoolOpts, SslOpts};
use mystique_connector_api::{
    ConnectionConfig, ConnectionFormSchema, ConnectorError, ConnectorResult, ConnectorSession,
    DatabaseConnector, EngineCapabilities, EngineFamily, FormField, FormFieldType, TestResult,
};

use crate::session::MysqlSession;

pub struct MysqlConnector;

fn format_error(err: impl std::fmt::Display) -> String {
    err.to_string()
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

fn build_opts(config: &ConnectionConfig) -> ConnectorResult<mysql_async::Opts> {
    let host = config
        .str_field("host")
        .unwrap_or_else(|| "127.0.0.1".into());
    let port = config.u16_field("port", 3306);
    let dbname = config.require_str("database")?;
    let user = config
        .str_field("username")
        .or_else(|| config.str_field("user"))
        .unwrap_or_else(|| "root".into());
    let password = config
        .password
        .clone()
        .or_else(|| config.str_field("password"));

    let max_size = config.pool.max_size.max(1) as usize;
    let min_idle = (config.pool.min_idle as usize).min(max_size);
    let constraints = PoolConstraints::new(min_idle, max_size).unwrap_or_else(|| {
        PoolConstraints::new(0, max_size.max(1)).expect("valid pool constraints")
    });
    let pool_opts = PoolOpts::default().with_constraints(constraints);

    // wait_timeout is server-side idle timeout in seconds (not connect timeout).
    let wait_secs = (config.pool.idle_timeout_secs.max(30) as usize).min(2_147_483);
    let mut builder = OptsBuilder::default()
        .ip_or_hostname(host)
        .tcp_port(port)
        .user(Some(user))
        .pass(password)
        .db_name(Some(dbname))
        .pool_opts(pool_opts)
        .wait_timeout(Some(wait_secs))
        .stmt_cache_size(0);

    if wants_ssl(config) {
        // Prefer TLS without verifying CA for local/dev; production can tighten later.
        builder = builder.ssl_opts(Some(SslOpts::default()));
    }

    Ok(builder.into())
}

async fn connect_once(config: &ConnectionConfig) -> ConnectorResult<(mysql_async::Conn, Pool)> {
    let opts = build_opts(config)?;
    let pool = Pool::new(opts);
    let conn = pool
        .get_conn()
        .await
        .map_err(|e| ConnectorError::Connect(format_error(e)))?;
    Ok((conn, pool))
}

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

    async fn test_connection(&self, config: &ConnectionConfig) -> ConnectorResult<TestResult> {
        let start = Instant::now();
        let (mut conn, pool) = connect_once(config).await?;

        let version: Option<String> = conn
            .query_first("SELECT VERSION()")
            .await
            .map_err(|e| ConnectorError::Query(format_error(e)))?;
        let latency_ms = start.elapsed().as_millis() as u64;

        drop(conn);
        pool.disconnect()
            .await
            .map_err(|e| ConnectorError::Connect(format_error(e)))?;

        Ok(TestResult {
            ok: true,
            engine: "mysql".into(),
            server_version: version,
            latency_ms,
            message: "Connected".into(),
        })
    }

    async fn connect(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<Box<dyn ConnectorSession>> {
        let opts = build_opts(config)?;
        let pool = Pool::new(opts);

        // Warm / validate: acquire one connection and ping.
        {
            let mut conn = pool
                .get_conn()
                .await
                .map_err(|e| ConnectorError::Connect(format_error(e)))?;
            conn.query_drop("SELECT 1")
                .await
                .map_err(|e| ConnectorError::Connect(format_error(e)))?;
        }

        Ok(Box::new(MysqlSession::new(pool, config.pool.max_size)))
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
            engine: "mysql".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 1,
                "database": "mystique",
                "username": "admin"
            }),
            password: Some("123".into()),
            pool: PoolConfig {
                max_size: 1,
                min_idle: 0,
                acquire_timeout_ms: 2000,
                idle_timeout_secs: 10,
            },
        };
        let connector = MysqlConnector;
        let err = connector
            .test_connection(&config)
            .await
            .expect_err("port 1 should refuse");
        let s = err.to_string();
        assert!(
            s.to_lowercase().contains("error")
                || s.to_lowercase().contains("connect")
                || s.to_lowercase().contains("refused")
                || s.to_lowercase().contains("connection"),
            "unexpected error shape: {s}"
        );
    }

    #[tokio::test]
    async fn test_connection_against_local_docker() {
        let config = ConnectionConfig {
            engine: "mysql".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 3306,
                "database": "mystique",
                "username": "admin"
            }),
            password: Some("123".into()),
            pool: PoolConfig {
                max_size: 2,
                min_idle: 0,
                acquire_timeout_ms: 5000,
                idle_timeout_secs: 10,
            },
        };
        let connector = MysqlConnector;
        let result = connector
            .test_connection(&config)
            .await
            .expect("connect to local mystique-mysql");
        assert!(result.ok, "msg={}", result.message);
        let ver = result.server_version.unwrap_or_default();
        assert!(
            ver.to_lowercase().contains("mysql") || ver.starts_with('8') || !ver.is_empty(),
            "version={ver}"
        );
    }
}
