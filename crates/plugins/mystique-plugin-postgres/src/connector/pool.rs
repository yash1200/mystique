use std::error::Error as StdError;
use std::time::Duration;

use deadpool_postgres::{Config as PoolCfg, ManagerConfig, RecyclingMethod, Runtime, SslMode};
use mystique_connector_api::{ConnectionConfig, ConnectorError, ConnectorResult};
use tokio_postgres::NoTls;

/// Flatten nested error sources so users see the real Postgres FATAL, not deadpool's "db error".
pub(super) fn format_error(err: &dyn StdError) -> String {
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

pub(super) fn wants_ssl(config: &ConnectionConfig) -> bool {
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

pub(crate) fn build_pool_config(config: &ConnectionConfig) -> ConnectorResult<PoolCfg> {
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

pub(super) fn build_pg_config(
    config: &ConnectionConfig,
) -> ConnectorResult<tokio_postgres::Config> {
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
    pg.application_name("Mystique");

    if wants_ssl(config) {
        pg.ssl_mode(tokio_postgres::config::SslMode::Require);
    } else {
        pg.ssl_mode(tokio_postgres::config::SslMode::Disable);
    }

    Ok(pg)
}

/// One-shot connect used by test_connection (clear errors, no pool wrapper).
pub(super) async fn connect_once(
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

pub(super) async fn create_pool(
    config: &ConnectionConfig,
) -> ConnectorResult<deadpool_postgres::Pool> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::PoolConfig;
    use serde_json::json;

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
}
