use mystique_connector_api::{
    ConnectionConfig, ConnectorError, ConnectorResult, FormattedConnectionUrl, ParsedConnectionUrl,
};
use tokio_postgres::config::{Host, SslMode as PgSslMode};

pub(crate) fn parse_postgres_url(url: &str) -> ConnectorResult<ParsedConnectionUrl> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "connection URL is empty".into(),
        ));
    }

    let ssl_mode_hint = sslmode_from_url(trimmed);
    let parseable = rewrite_sslmode_for_driver(trimmed);
    let cfg: tokio_postgres::Config = parseable.parse().map_err(|e| {
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
        .or_else(|| cfg.get_hostaddrs().first().map(|ip| ip.to_string()))
        .unwrap_or_else(|| "127.0.0.1".into());

    let port = cfg.get_ports().first().copied().unwrap_or(5432);
    let database = cfg.get_dbname().unwrap_or("postgres").to_string();
    let username = cfg.get_user().unwrap_or("postgres").to_string();

    let password = cfg
        .get_password()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned());

    let ssl_mode = ssl_mode_hint.unwrap_or_else(|| match cfg.get_ssl_mode() {
        PgSslMode::Require => "require",
        PgSslMode::Prefer => "prefer",
        _ => "disable",
    });
    let ssl = ssl_mode != "disable";

    Ok(ParsedConnectionUrl {
        engine: "postgres".into(),
        plugin_config: serde_json::json!({
            "host": host,
            "port": port,
            "database": database,
            "username": username,
            "ssl": ssl,
            "sslMode": ssl_mode,
        }),
        password,
    })
}

fn rewrite_sslmode_for_driver(url: &str) -> String {
    // tokio-postgres accepts disable|prefer|require, not verify-ca|verify-full.
    url.replace("sslmode=verify-full", "sslmode=require")
        .replace("sslmode=verify-ca", "sslmode=require")
        .replace("sslmode=verify_full", "sslmode=require")
        .replace("sslmode=verify_ca", "sslmode=require")
}

fn sslmode_from_url(url: &str) -> Option<&'static str> {
    let query = url.split_once('?')?.1;
    for part in query.split(['&', '#']) {
        let (k, v) = part.split_once('=')?;
        if k.eq_ignore_ascii_case("sslmode") {
            return Some(match v.trim().to_ascii_lowercase().as_str() {
                "prefer" => "prefer",
                "require" => "require",
                "verify-ca" => "verify-ca",
                "verify-full" => "verify-full",
                _ => "disable",
            });
        }
    }
    None
}

/// Format discrete plugin config into a `postgresql://` URL (clipboard / share).
pub(crate) fn format_postgres_url(
    config: &ConnectionConfig,
) -> ConnectorResult<FormattedConnectionUrl> {
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
    let ssl_mode = mystique_tls::SslMode::from_config(config);

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
    let qs = if ssl_mode.encrypts() {
        format!("?sslmode={}", ssl_mode.as_str())
    } else {
        String::new()
    };
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

pub(super) fn encode_url_component(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::PoolConfig;
    use serde_json::json;

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
    fn parse_connection_url_verify_full() {
        let parsed =
            parse_postgres_url("postgresql://app@db.example.com/appdb?sslmode=verify-full")
                .expect("parse");
        assert_eq!(parsed.plugin_config["ssl"], true);
        assert_eq!(parsed.plugin_config["sslMode"], "verify-full");
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
}
