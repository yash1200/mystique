use mysql_async::{ClientIdentity, OptsBuilder, Pool, PoolConstraints, PoolOpts, SslOpts};
use mystique_connector_api::{
    prettify_sql_error, ConnectionConfig, ConnectorError, ConnectorResult,
};
use mystique_tls::TlsSettings;

pub(crate) fn format_error(err: &mysql_async::Error) -> String {
    match err {
        mysql_async::Error::Server(s) => {
            if let Some(msg) = pretty_mysql_code(s.code, &s.message) {
                return msg;
            }
            let msg = s.message.trim();
            if msg.is_empty() {
                format!("MySQL error {} ({})", s.code, s.state)
            } else {
                prettify_sql_error(msg)
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

/// MySQL names `at line N` and often a `near '…'` snippet. No column number.
pub(crate) fn query_failed_at(sql: &str, err: mysql_async::Error) -> ConnectorError {
    // The offset comes from the server text. `format_error` may rewrite that
    // text and drop `at line` / `near`.
    let raw = server_message(&err);
    let message = format_error(&err);
    let Some(offset) = raw
        .as_deref()
        .and_then(|text| mysql_local_offset(sql, text))
    else {
        return ConnectorError::Query(message);
    };
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

fn server_message(err: &mysql_async::Error) -> Option<String> {
    match err {
        mysql_async::Error::Server(server) => Some(server.message.clone()),
        _ => None,
    }
}

pub(crate) fn mysql_local_offset(sql: &str, message: &str) -> Option<u32> {
    let line = mysql_line(message)?;
    let line_start = start_of_line(sql, line);
    if let Some(near) = near_token(message) {
        let window = sql.get(line_start..).unwrap_or("");
        if let Some(found) = window.find(near) {
            return u32::try_from(line_start + found).ok();
        }
        if let Some(found) = sql.find(near) {
            return u32::try_from(found).ok();
        }
    }
    u32::try_from(line_start).ok()
}

fn mysql_line(message: &str) -> Option<u32> {
    let lower = message.to_ascii_lowercase();
    let marker = "at line ";
    let at = lower.rfind(marker)?;
    let rest = message.get(at.saturating_add(marker.len())..)?.trim_start();
    let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    let line = digits.parse().ok()?;
    if line == 0 {
        None
    } else {
        Some(line)
    }
}

fn start_of_line(sql: &str, line: u32) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut current = 1u32;
    for (index, ch) in sql.char_indices() {
        if current == line {
            return index;
        }
        if ch == '\n' {
            current += 1;
        }
    }
    sql.len()
}

fn near_token(message: &str) -> Option<&str> {
    let lower = message.to_ascii_lowercase();
    let marker = "near '";
    let at = lower.rfind(marker)?;
    let rest = message.get(at.saturating_add(marker.len())..)?;
    let end = rest.find('\'')?;
    let token = rest.get(..end)?.trim();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn pretty_mysql_code(code: u16, message: &str) -> Option<String> {
    match code {
        1048 | 1364 => Some(prettify_sql_error(message)),
        1062 => Some(prettify_sql_error(message)),
        1451 => Some("This row is still used by another table.".into()),
        1452 => Some("This value doesn't match a related row.".into()),
        1406 | 1264 => Some(prettify_sql_error(message)),
        1366 => Some("This value is the wrong type.".into()),
        _ => None,
    }
}

pub(super) fn wants_ssl(config: &ConnectionConfig) -> bool {
    TlsSettings::from_config(config).enabled()
}

fn ssl_opts(settings: &TlsSettings, hostname_override: Option<&str>) -> ConnectorResult<SslOpts> {
    let mut opts = SslOpts::default()
        .with_danger_accept_invalid_certs(!settings.mode.verify_certs())
        .with_danger_skip_domain_validation(!settings.mode.verify_hostname());
    if let Some(name) = hostname_override.filter(|s| !s.is_empty()) {
        opts = opts.with_danger_tls_hostname_override(Some(name.to_string()));
    }
    if let Some(ca) = &settings.ca_cert_path {
        opts = opts
            .with_root_certs(vec![ca.clone().into()])
            .with_disable_built_in_roots(true);
    }
    let identity_path = settings
        .client_key_path
        .as_ref()
        .or(settings.client_cert_path.as_ref());
    if let Some(path) = identity_path {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "p12" && ext != "pfx" {
            return Err(ConnectorError::InvalidConfig(
                "MySQL client certificate must be a PKCS#12 (.p12 or .pfx) file.".into(),
            ));
        }
        let mut identity = ClientIdentity::new(path.clone().into());
        if let Some(pw) = &settings.client_key_passphrase {
            identity = identity.with_password(pw.clone());
        }
        opts = opts.with_client_identity(Some(identity));
    }
    Ok(opts)
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
    let constraints = PoolConstraints::new(min_idle, max_size)
        .or_else(|| PoolConstraints::new(0, max_size.max(1)))
        .ok_or_else(|| ConnectorError::InvalidConfig("invalid MySQL pool size".into()))?;
    // Checkout does not ping. There is no "skip ping if verified recently" knob.
    // `reset_connection` stays at the driver default (on) so session variables
    // and an open transaction cannot leak to the next checkout.
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

    let tls = TlsSettings::from_config(config);
    tls.check(&config.engine)?;
    if tls.enabled() {
        let tls_name = config.str_field("tlsServerName");
        builder = builder.ssl_opts(Some(ssl_opts(&tls, tls_name.as_deref())?));
    }

    Ok(builder.into())
}

/// Same server and credentials, for grid saves. `CLIENT_FOUND_ROWS` makes an
/// UPDATE that matches a row but changes nothing report 1, not 0.
pub(super) fn edit_opts(opts: &mysql_async::Opts) -> mysql_async::Opts {
    let constraints = PoolConstraints::new(0, 2).unwrap_or_default();
    OptsBuilder::from_opts(opts.clone())
        .client_found_rows(true)
        .pool_opts(PoolOpts::default().with_constraints(constraints))
        .into()
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
mod offset_tests {
    use super::mysql_local_offset;

    #[test]
    fn near_token_on_line_two_is_not_the_start_of_the_buffer() {
        let sql = "SELECT 1;\nSELECT * FORM t";
        let message = "You have an error in your SQL syntax near 'FORM t' at line 2";
        let offset = mysql_local_offset(sql, message).expect("offset");
        let at = usize::try_from(offset).unwrap_or(0);
        assert!(sql.get(at..).unwrap_or("").starts_with("FORM"), "{offset}");
    }

    #[test]
    fn a_message_without_a_line_has_no_position() {
        assert!(mysql_local_offset("SELECT 1", "Duplicate entry").is_none());
    }

    #[test]
    fn syntax_error_points_at_the_near_token() {
        let sql = "SELECT * FORM t";
        let err = mysql_async::Error::Server(mysql_async::ServerError {
            code: 1064,
            message: "You have an error in your SQL syntax near 'FORM t' at line 1".into(),
            state: "42000".into(),
        });
        let mapped = super::query_failed_at(sql, err);
        let mystique_connector_api::ConnectorError::QueryAt { position, .. } = mapped else {
            panic!("expected a position, got {mapped}");
        };
        let at = usize::try_from(position.offset).unwrap_or(sql.len());
        assert!(
            sql.get(at..).is_some_and(|rest| rest.starts_with("FORM")),
            "{at}"
        );
    }

    #[test]
    fn duplicate_entry_has_no_position() {
        let err = mysql_async::Error::Server(mysql_async::ServerError {
            code: 1062,
            message: "Duplicate entry 'a' for key 't.PRIMARY'".into(),
            state: "23000".into(),
        });
        let mapped = super::query_failed_at("INSERT INTO t VALUES (1)", err);
        assert!(
            matches!(mapped, mystique_connector_api::ConnectorError::Query(_)),
            "{mapped}"
        );
    }
}
#[cfg(test)]
mod tests {
    use super::format_error;
    use mysql_async::{Error, ServerError};

    #[test]
    fn prefer_is_rejected() {
        let err = super::build_opts(&mystique_connector_api::ConnectionConfig {
            engine: "mysql".into(),
            plugin_config: serde_json::json!({
                "host": "127.0.0.1",
                "database": "app",
                "sslMode": "prefer"
            }),
            password: None,
            pool: mystique_connector_api::PoolConfig::default(),
        })
        .unwrap_err();
        assert!(err.to_string().contains("cannot try TLS"), "{err}");
    }

    #[test]
    fn verify_full_through_ssh_overrides_the_tls_name() {
        let opts = super::build_opts(&mystique_connector_api::ConnectionConfig {
            engine: "mariadb".into(),
            plugin_config: serde_json::json!({
                "host": "127.0.0.1",
                "port": 6000,
                "database": "app",
                "sslMode": "require",
                "tlsServerName": "db.internal"
            }),
            password: None,
            pool: mystique_connector_api::PoolConfig::default(),
        })
        .expect("opts");
        assert_eq!(opts.ip_or_hostname(), "127.0.0.1");
        assert_eq!(
            opts.ssl_opts().and_then(|ssl| ssl.tls_hostname_override()),
            Some("db.internal")
        );
    }

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

    #[test]
    fn not_null_names_the_column() {
        let err = Error::Server(ServerError {
            code: 1048,
            message: "Column 'content' cannot be null".into(),
            state: "23000".into(),
        });
        assert_eq!(format_error(&err), "\"content\" cannot be empty.");
    }
}
