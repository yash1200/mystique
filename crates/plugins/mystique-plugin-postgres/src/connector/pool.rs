use std::error::Error as StdError;
use std::time::Duration;

use deadpool_postgres::{Config as PoolCfg, ManagerConfig, RecyclingMethod, Runtime, SslMode};
use mystique_connector_api::{
    prettify_sql_error, ConnectionConfig, ConnectorError, ConnectorResult,
};
use tokio_postgres::error::{DbError, SqlState};
use tokio_postgres::NoTls;

/// Driver wrappers say "db error"; the useful text is on [`DbError`].
pub(crate) fn format_error(err: &(dyn StdError + 'static)) -> String {
    if let Some(db) = find_db_error(err) {
        return format_db_error(db);
    }
    let mut parts = Vec::new();
    let mut cur: Option<&(dyn StdError + 'static)> = Some(err);
    while let Some(e) = cur {
        let s = e.to_string();
        if !is_noise(&s) && parts.last().map(|p: &String| p != &s).unwrap_or(true) {
            parts.push(s);
        }
        cur = e.source();
    }
    if parts.is_empty() {
        "PostgreSQL could not run this statement.".into()
    } else {
        parts.join(" — ")
    }
}

pub(crate) fn query_failed(err: tokio_postgres::Error) -> ConnectorError {
    ConnectorError::Query(format_error(&err))
}

fn find_db_error<'a>(err: &'a (dyn StdError + 'static)) -> Option<&'a DbError> {
    let mut cur = Some(err);
    while let Some(e) = cur {
        if let Some(pg) = e.downcast_ref::<tokio_postgres::Error>() {
            if let Some(db) = pg.as_db_error() {
                return Some(db);
            }
        }
        if let Some(db) = e.downcast_ref::<DbError>() {
            return Some(db);
        }
        cur = e.source();
    }
    None
}

fn format_db_error(db: &DbError) -> String {
    if let Some(msg) = pretty_from_sqlstate(db) {
        return msg;
    }
    prettify_sql_error(db.message())
}

fn pretty_from_sqlstate(db: &DbError) -> Option<String> {
    let col = db.column().map(str::to_string);
    match *db.code() {
        SqlState::NOT_NULL_VIOLATION => Some(match col {
            Some(c) => format!("\"{c}\" cannot be empty."),
            None => prettify_sql_error(db.message()),
        }),
        SqlState::UNIQUE_VIOLATION => Some(match col {
            Some(c) => format!("A row with this \"{c}\" already exists."),
            None => "A row with this value already exists.".into(),
        }),
        SqlState::FOREIGN_KEY_VIOLATION => {
            let msg = db.message().to_ascii_lowercase();
            if msg.contains("is still referenced") {
                Some("This row is still used by another table.".into())
            } else {
                Some("This value doesn't match a related row.".into())
            }
        }
        SqlState::CHECK_VIOLATION => Some("This value doesn't meet a check constraint.".into()),
        SqlState::STRING_DATA_RIGHT_TRUNCATION => Some("This value is too long.".into()),
        SqlState::INVALID_TEXT_REPRESENTATION | SqlState::DATATYPE_MISMATCH => {
            Some("This value is the wrong type.".into())
        }
        _ => None,
    }
}

fn is_noise(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "db error" | "error occurred in the database" | "unknown error"
    )
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

    #[test]
    fn not_null_message_is_short() {
        let raw = "null value in column \"content\" of relation \"session_messages\" \
            violates not-null constraint. Failing row contains (1, null).";
        assert_eq!(
            mystique_connector_api::prettify_sql_error(raw),
            "\"content\" cannot be empty."
        );
    }
}
