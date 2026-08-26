use mysql_async::Opts;
use mystique_connector_api::{
    ConnectionConfig, ConnectorError, ConnectorResult, FormattedConnectionUrl, ParsedConnectionUrl,
};

use super::pool::wants_ssl;

/// Normalize `mariadb://` to `mysql://` so mysql_async’s URL parser accepts it.
pub(super) fn normalize_mysql_url(url: &str) -> String {
    let t = url.trim();
    if let Some(rest) = t
        .strip_prefix("mariadb://")
        .or_else(|| t.strip_prefix("MARIADB://"))
    {
        format!("mysql://{rest}")
    } else {
        t.to_string()
    }
}

/// Parse `mysql://` / `mariadb://` into discrete form fields via mysql_async.
pub(crate) fn parse_mysql_url(url: &str) -> ConnectorResult<ParsedConnectionUrl> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "connection URL is empty".into(),
        ));
    }

    let normalized = normalize_mysql_url(trimmed);
    let opts = Opts::from_url(&normalized)
        .map_err(|e| ConnectorError::InvalidConfig(format!("invalid MySQL connection URL: {e}")))?;

    let host = opts.ip_or_hostname().to_string();
    let port = opts.tcp_port();
    let database = opts
        .db_name()
        .filter(|s| !s.is_empty())
        .unwrap_or("mystique")
        .to_string();
    let username = opts
        .user()
        .filter(|s| !s.is_empty())
        .unwrap_or("admin")
        .to_string();
    let password = opts.pass().map(|s| s.to_string());
    let ssl = opts.ssl_opts().is_some();

    Ok(ParsedConnectionUrl {
        engine: "mysql".into(),
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

/// Format discrete plugin config into a `mysql://` URL (clipboard / share).
pub(crate) fn format_mysql_url(
    config: &ConnectionConfig,
) -> ConnectorResult<FormattedConnectionUrl> {
    let host = config
        .str_field("host")
        .unwrap_or_else(|| "127.0.0.1".into());
    let port = config.u16_field("port", 3306);
    let database = config
        .str_field("database")
        .unwrap_or_else(|| "mystique".into());
    let user = config
        .str_field("username")
        .or_else(|| config.str_field("user"))
        .unwrap_or_else(|| "admin".into());
    let password = config
        .password
        .clone()
        .or_else(|| config.str_field("password"))
        .filter(|s| !s.is_empty());
    let ssl = wants_ssl(config);

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
    let qs = if ssl { "?require_ssl=true" } else { "" };
    let url = format!("mysql://{authority}{path}{qs}");

    // Round-trip through mysql_async so we never emit an unusable URL.
    let _ = Opts::from_url(&url)
        .map_err(|e| ConnectorError::InvalidConfig(format!("failed to format MySQL URL: {e}")))?;

    Ok(FormattedConnectionUrl {
        engine: "mysql".into(),
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

    #[test]
    fn parse_connection_url_mysql_scheme() {
        let parsed = parse_mysql_url("mysql://admin:123@127.0.0.1:3307/appdb?require_ssl=true")
            .expect("parse");
        assert_eq!(parsed.engine, "mysql");
        assert_eq!(parsed.password.as_deref(), Some("123"));
        let cfg = &parsed.plugin_config;
        assert_eq!(cfg["host"], "127.0.0.1");
        assert_eq!(cfg["port"], 3307);
        assert_eq!(cfg["database"], "appdb");
        assert_eq!(cfg["username"], "admin");
        assert_eq!(cfg["ssl"], true);
    }

    #[test]
    fn parse_connection_url_mariadb_scheme() {
        let parsed = parse_mysql_url("mariadb://root@db.local/mydb").expect("parse");
        assert_eq!(parsed.plugin_config["host"], "db.local");
        assert_eq!(parsed.plugin_config["database"], "mydb");
        assert_eq!(parsed.plugin_config["username"], "root");
        assert_eq!(parsed.plugin_config["port"], 3306);
    }

    #[test]
    fn parse_connection_url_rejects_empty() {
        let err = parse_mysql_url("").unwrap_err();
        assert!(err.to_string().to_lowercase().contains("empty"), "{err}");
    }

    #[test]
    fn format_connection_url_round_trip() {
        let config = ConnectionConfig {
            engine: "mysql".into(),
            plugin_config: serde_json::json!({
                "host": "127.0.0.1",
                "port": 3307,
                "database": "appdb",
                "username": "admin",
                "ssl": true,
                "sslMode": "require"
            }),
            password: Some("123".into()),
            pool: PoolConfig::default(),
        };
        let formatted = format_mysql_url(&config).expect("format");
        assert!(
            formatted.url.starts_with("mysql://"),
            "url={}",
            formatted.url
        );
        let parsed = parse_mysql_url(&formatted.url).expect("re-parse");
        assert_eq!(parsed.plugin_config["host"], "127.0.0.1");
        assert_eq!(parsed.plugin_config["port"], 3307);
        assert_eq!(parsed.plugin_config["database"], "appdb");
        assert_eq!(parsed.plugin_config["username"], "admin");
        assert_eq!(parsed.password.as_deref(), Some("123"));
        assert_eq!(parsed.plugin_config["ssl"], true);
    }
}
