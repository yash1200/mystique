use mysql_async::{OptsBuilder, Pool, PoolConstraints, PoolOpts, SslOpts};
use mystique_connector_api::{ConnectionConfig, ConnectorError, ConnectorResult};

pub(crate) fn format_error(err: &mysql_async::Error) -> String {
    match err {
        mysql_async::Error::Server(s) => {
            let msg = s.message.trim();
            if msg.is_empty() {
                format!("MySQL error {} ({})", s.code, s.state)
            } else {
                msg.to_string()
            }
        }
        mysql_async::Error::Io(io) => format!("Could not reach MySQL: {io}"),
        mysql_async::Error::Driver(d) => d.to_string(),
        mysql_async::Error::Url(u) => u.to_string(),
        mysql_async::Error::Other(o) => o.to_string(),
    }
}

pub(crate) fn query_failed(err: mysql_async::Error) -> ConnectorError {
    ConnectorError::Query(format_error(&err))
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

pub(super) fn build_opts(config: &ConnectionConfig) -> ConnectorResult<mysql_async::Opts> {
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

pub(super) async fn connect_once(
    config: &ConnectionConfig,
) -> ConnectorResult<(mysql_async::Conn, Pool)> {
    let opts = build_opts(config)?;
    let pool = Pool::new(opts);
    let conn = pool
        .get_conn()
        .await
        .map_err(|e| ConnectorError::Connect(format_error(&e)))?;
    Ok((conn, pool))
}

#[cfg(test)]
mod tests {
    use super::format_error;
    use mysql_async::{Error, ServerError};

    #[test]
    fn server_error_uses_mysql_message() {
        let err = Error::Server(ServerError {
            code: 1064,
            message: "You have an error in your SQL syntax near 'SELEC'".into(),
            state: "42000".into(),
        });
        assert_eq!(
            format_error(&err),
            "You have an error in your SQL syntax near 'SELEC'"
        );
    }
}
