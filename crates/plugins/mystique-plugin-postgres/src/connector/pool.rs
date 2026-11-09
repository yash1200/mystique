use std::error::Error as StdError;
use std::net::IpAddr;
use std::time::Duration;

use deadpool_postgres::{Config as PoolCfg, ManagerConfig, RecyclingMethod, Runtime, SslMode};
use mystique_connector_api::{
    prettify_sql_error, ConnectionConfig, ConnectorError, ConnectorResult,
};
use mystique_tls::{SslMode as MystSslMode, TlsSettings};
use tokio_postgres::error::{DbError, SqlState};
use tokio_postgres::NoTls;

use super::notices::{NoticeConnect, NoticeHub};

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

/// Same text as [`query_failed`], plus a byte offset when Postgres names a
/// character in `sql`. Internal positions (a server-rewritten query) are omitted.
pub(crate) fn query_failed_at(sql: &str, err: tokio_postgres::Error) -> ConnectorError {
    let Some(db) = find_db_error(&err) else {
        return ConnectorError::Query(format_error(&err));
    };
    let message = format_db_error(db);
    let Some(tokio_postgres::error::ErrorPosition::Original(pos)) = db.position() else {
        return ConnectorError::Query(message);
    };
    if *pos == 0 {
        return ConnectorError::Query(message);
    }
    let offset = char_to_byte(sql, pos.saturating_sub(1));
    ConnectorError::query_at(
        message,
        mystique_connector_api::SourcePosition {
            offset,
            line: 1,
            column: 1,
        },
        None,
    )
}

fn char_to_byte(sql: &str, char_index: u32) -> u32 {
    let byte = usize::try_from(char_index)
        .ok()
        .and_then(|n| sql.char_indices().nth(n))
        .map_or(sql.len(), |(index, _)| index);
    u32::try_from(byte).unwrap_or(u32::MAX)
}

/// `DECLARE … FOR <body>` reports positions in the wrapped text.
pub(crate) fn shift_query_position(err: ConnectorError, prefix_bytes: u32) -> ConnectorError {
    let ConnectorError::QueryAt {
        message,
        position,
        statement_index,
    } = err
    else {
        return err;
    };
    if position.offset < prefix_bytes {
        return ConnectorError::Query(message);
    }
    ConnectorError::query_at(
        message,
        mystique_connector_api::SourcePosition {
            offset: position.offset - prefix_bytes,
            line: 1,
            column: 1,
        },
        statement_index,
    )
}

/// `Row::get` panics on NULL or a width mismatch (Cockroach INT8 vs INT4).
pub(crate) fn pg_col<'a, T>(row: &'a tokio_postgres::Row, idx: usize) -> ConnectorResult<T>
where
    T: tokio_postgres::types::FromSql<'a>,
{
    row.try_get(idx)
        .map_err(|err| ConnectorError::Query(format!("Could not read column {idx}: {err}")))
}

/// Length and precision columns are int4 on Postgres and int8 on Cockroach.
pub(crate) fn pg_i32_col(row: &tokio_postgres::Row, idx: usize) -> ConnectorResult<Option<i32>> {
    match row.try_get::<_, Option<i32>>(idx) {
        Ok(value) => Ok(value),
        Err(err) => match row.try_get::<_, Option<i64>>(idx) {
            Ok(value) => value
                .map(i32::try_from)
                .transpose()
                .map_err(|_| ConnectorError::Query(format!("Column {idx} does not fit in int4."))),
            Err(_) => Err(ConnectorError::Query(format!(
                "Could not read column {idx}: {err}"
            ))),
        },
    }
}

/// `oid` is u32 on Postgres and may arrive as int8 on Cockroach.
pub(crate) fn pg_u32_col(row: &tokio_postgres::Row, idx: usize) -> ConnectorResult<u32> {
    if let Ok(value) = row.try_get::<_, u32>(idx) {
        return Ok(value);
    }
    if let Ok(value) = row.try_get::<_, i64>(idx) {
        return u32::try_from(value)
            .map_err(|_| ConnectorError::Query(format!("Column {idx} does not fit in u32.")));
    }
    let value: i32 = pg_col(row, idx)?;
    u32::try_from(value)
        .map_err(|_| ConnectorError::Query(format!("Column {idx} does not fit in u32.")))
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

pub(crate) fn build_pool_config(config: &ConnectionConfig) -> ConnectorResult<PoolCfg> {
    let dial = dial_target(config);
    TlsSettings::from_config(config).check(&config.engine)?;
    let dbname = config.require_str("database")?;
    let user = config
        .str_field("username")
        .or_else(|| config.str_field("user"))
        .unwrap_or_else(|| "postgres".into());

    let mut cfg = PoolCfg::new();
    cfg.host = Some(dial.tls_name.clone());
    cfg.hostaddr = dial.addr;
    cfg.port = Some(dial.port);
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

    cfg.ssl_mode = Some(deadpool_ssl_mode(&TlsSettings::from_config(config)));

    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    let mut pool = deadpool_postgres::PoolConfig::new(config.pool.max_size.max(1) as usize);
    pool.timeouts.wait = Some(Duration::from_millis(timeout_ms));
    pool.timeouts.create = Some(Duration::from_millis(timeout_ms));
    // deadpool's `Timeouts` is wait / create / recycle. There is no idle field.
    // `recycle` is how long a checkout may spend recycling, not how long a
    // connection may sit idle, so `idle_timeout_secs` must not be copied onto it.
    // TCP keepalives are what notice a dead server.
    cfg.keepalives = Some(true);
    cfg.keepalives_idle = Some(Duration::from_secs(30));
    cfg.pool = Some(pool);
    Ok(cfg)
}

pub(super) fn build_pg_config(
    config: &ConnectionConfig,
) -> ConnectorResult<tokio_postgres::Config> {
    let dial = dial_target(config);
    TlsSettings::from_config(config).check(&config.engine)?;
    let dbname = config.require_str("database")?;
    let user = config
        .str_field("username")
        .or_else(|| config.str_field("user"))
        .unwrap_or_else(|| "postgres".into());

    let mut pg = tokio_postgres::Config::new();
    pg.host(&dial.tls_name);
    if let Some(addr) = dial.addr {
        pg.hostaddr(addr);
    }
    pg.port(dial.port);
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
    pg.keepalives(true);
    pg.keepalives_idle(Duration::from_secs(30));

    pg.ssl_mode(tokio_ssl_mode(&TlsSettings::from_config(config)));

    Ok(pg)
}

/// One-shot connect used by test_connection (clear errors, no pool wrapper).
pub(super) async fn connect_once(
    config: &ConnectionConfig,
) -> ConnectorResult<(tokio_postgres::Client, tokio::task::JoinHandle<()>)> {
    let pg = build_pg_config(config)?;

    if let Some(tls_connector) = TlsSettings::from_config(config).native_connector()? {
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

/// TLS connector kept for `CancelToken::cancel_query`. Plain sockets use `NoTls`.
#[derive(Clone)]
pub(crate) enum PgCancelTls {
    Plain(NoTls),
    Native(postgres_native_tls::MakeTlsConnector),
}

impl PgCancelTls {
    pub(crate) async fn cancel_query(&self, token: tokio_postgres::CancelToken) {
        let result = match self {
            Self::Plain(tls) => token.cancel_query(*tls).await,
            Self::Native(tls) => token.cancel_query(tls.clone()).await,
        };
        let _ = result;
    }
}

pub(super) async fn create_pool(
    config: &ConnectionConfig,
) -> ConnectorResult<(deadpool_postgres::Pool, PgCancelTls, NoticeHub)> {
    let pool_cfg = build_pool_config(config)?;
    let hub = NoticeHub::default();
    if let Some(tls_connector) = TlsSettings::from_config(config).native_connector()? {
        let tls = postgres_native_tls::MakeTlsConnector::new(tls_connector);
        let pool = build_pool(&pool_cfg, tls.clone(), hub.clone())?;
        Ok((pool, PgCancelTls::Native(tls), hub))
    } else {
        let pool = build_pool(&pool_cfg, NoTls, hub.clone())?;
        Ok((pool, PgCancelTls::Plain(NoTls), hub))
    }
}

/// `PoolCfg::create_pool` with [`NoticeConnect`] in place of deadpool's
/// connect, so server notices reach the run that raised them.
fn build_pool<T>(
    pool_cfg: &PoolCfg,
    tls: T,
    hub: NoticeHub,
) -> ConnectorResult<deadpool_postgres::Pool>
where
    T: tokio_postgres::tls::MakeTlsConnect<tokio_postgres::Socket> + Clone + Sync + Send + 'static,
    T::Stream: Sync + Send,
    T::TlsConnect: Sync + Send,
    <T::TlsConnect as tokio_postgres::tls::TlsConnect<tokio_postgres::Socket>>::Future: Send,
{
    let pg_config = pool_cfg
        .get_pg_config()
        .map_err(|e| ConnectorError::Pool(format_error(&e)))?;
    let manager = deadpool_postgres::Manager::from_connect(
        pg_config,
        NoticeConnect { tls, hub },
        pool_cfg.get_manager_config(),
    );
    deadpool_postgres::Pool::builder(manager)
        .config(pool_cfg.get_pool_config())
        .runtime(Runtime::Tokio1)
        .build()
        .map_err(|e| ConnectorError::Pool(format_error(&e)))
}

struct DialTarget {
    /// Name checked by verify-full. The real server, not the tunnel.
    tls_name: String,
    /// Set when the TCP dial address is not the TLS name (SSH local forward).
    addr: Option<IpAddr>,
    port: u16,
}

fn dial_target(config: &ConnectionConfig) -> DialTarget {
    let host = config
        .str_field("host")
        .unwrap_or_else(|| "127.0.0.1".into());
    let port = config.u16_field("port", 5432);
    let tls_name = config.str_field("tlsServerName").filter(|s| !s.is_empty());
    match tls_name {
        Some(name) => DialTarget {
            tls_name: name,
            addr: host.parse().ok(),
            port,
        },
        None => DialTarget {
            tls_name: host,
            addr: None,
            port,
        },
    }
}

fn tokio_ssl_mode(settings: &TlsSettings) -> tokio_postgres::config::SslMode {
    match settings.mode {
        MystSslMode::Disable => tokio_postgres::config::SslMode::Disable,
        MystSslMode::Prefer => tokio_postgres::config::SslMode::Prefer,
        _ => tokio_postgres::config::SslMode::Require,
    }
}

fn deadpool_ssl_mode(settings: &TlsSettings) -> SslMode {
    match settings.mode {
        MystSslMode::Disable => SslMode::Disable,
        MystSslMode::Prefer => SslMode::Prefer,
        _ => SslMode::Require,
    }
}

#[cfg(test)]
mod position_tests {
    use super::{char_to_byte, shift_query_position};
    use mystique_connector_api::ConnectorError;

    #[test]
    fn a_multibyte_character_index_lands_on_that_character() {
        let sql = "SELECT '你'";
        let offset = char_to_byte(sql, 8);
        let at = usize::try_from(offset).unwrap_or(sql.len());
        assert_eq!(sql.get(at..at.saturating_add('你'.len_utf8())), Some("你"));
    }

    #[test]
    fn declare_prefix_moves_the_offset_onto_the_user_body() {
        let body = "SELECT * FORM t";
        let prefix = "DECLARE mystique_row_cap NO SCROLL CURSOR FOR ";
        let declare = format!("{prefix}{body}");
        let Some(form) = declare.find("FORM") else {
            panic!("FORM missing");
        };
        let Ok(form_at) = u32::try_from(form) else {
            panic!("offset");
        };
        let err = ConnectorError::query_at(
            "syntax",
            mystique_connector_api::position_at(&declare, form_at),
            None,
        );
        let Ok(prefix_len) = u32::try_from(prefix.len()) else {
            panic!("prefix");
        };
        let shifted = shift_query_position(err, prefix_len);
        let ConnectorError::QueryAt { position, .. } = shifted else {
            panic!("expected a position");
        };
        let at = usize::try_from(position.offset).unwrap_or(body.len());
        assert_eq!(body.get(at..at.saturating_add(4)), Some("FORM"));
    }

    #[test]
    fn a_position_inside_the_declare_prefix_is_not_an_underline() {
        let err = ConnectorError::query_at(
            "syntax",
            mystique_connector_api::position_at("DECLARE x", 0),
            None,
        );
        let shifted = shift_query_position(err, 40);
        assert!(matches!(shifted, ConnectorError::Query(_)));
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
        assert!(cfg
            .pool
            .as_ref()
            .and_then(|pool| pool.timeouts.recycle)
            .is_none());
        assert_eq!(cfg.keepalives, Some(true));
        assert_eq!(cfg.keepalives_idle, Some(Duration::from_secs(30)));
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
    fn verify_full_through_ssh_checks_the_real_hostname() {
        let config = ConnectionConfig {
            engine: "postgres".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 5555,
                "tlsServerName": "db.internal",
                "database": "app",
                "sslMode": "verify-full",
                "caCertPath": "/tmp/mystique-ca.pem"
            }),
            password: None,
            pool: PoolConfig::default(),
        };
        let pg = build_pg_config(&config).expect("pg config");
        assert_eq!(
            pg.get_hosts(),
            &[tokio_postgres::config::Host::Tcp("db.internal".into())]
        );
        assert_eq!(
            pg.get_hostaddrs(),
            &[std::net::IpAddr::from([127, 0, 0, 1])]
        );
        assert_eq!(pg.get_ports(), &[5555]);
        assert!(pg.get_keepalives());
        assert_eq!(pg.get_keepalives_idle(), Duration::from_secs(30));
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
