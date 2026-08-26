use mystique_connector_api::{
    ConnectionConfig, ConnectorError, ConnectorResult, FormattedConnectionUrl,
};

/// Extract `/dbname` from a mongodb:// or mongodb+srv:// URI (before `?` / `#`).
pub(crate) fn database_from_mongo_uri(uri: &str) -> Option<String> {
    let rest = uri
        .strip_prefix("mongodb://")
        .or_else(|| uri.strip_prefix("mongodb+srv://"))?;
    // Authority ends at first `/` that starts the path (not credentials@host).
    let after_at = rest
        .rsplit_once('@')
        .map(|(_, host_path)| host_path)
        .unwrap_or(rest);
    let path = after_at.split_once('/')?.1;
    let name = path.split(['?', '#']).next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
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

/// Prefer stored URI; otherwise assemble `mongodb://` from discrete fields.
pub(crate) fn format_mongo_url(config: &ConnectionConfig) -> ConnectorResult<FormattedConnectionUrl> {
    if let Some(uri) = config.str_field("uri") {
        let uri = uri.trim().to_string();
        if uri.is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "connection URI is empty".into(),
            ));
        }
        return Ok(FormattedConnectionUrl {
            engine: "mongodb".into(),
            url: uri,
        });
    }

    let host = config
        .str_field("host")
        .unwrap_or_else(|| "127.0.0.1".into());
    let port = config.u16_field("port", 27017);
    let database = config.str_field("database").unwrap_or_default();
    let user = config
        .str_field("username")
        .or_else(|| config.str_field("user"))
        .unwrap_or_default();
    let password = config
        .password
        .clone()
        .or_else(|| config.str_field("password"))
        .filter(|s| !s.is_empty());
    let auth_source = config
        .str_field("authSource")
        .or_else(|| config.str_field("auth_source"))
        .unwrap_or_else(|| {
            if user.is_empty() {
                String::new()
            } else {
                "admin".into()
            }
        });

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
        "/".to_string()
    } else {
        format!("/{}", encode_url_component(&database))
    };
    let qs = if auth_source.is_empty() {
        String::new()
    } else {
        format!("?authSource={}", encode_url_component(&auth_source))
    };

    Ok(FormattedConnectionUrl {
        engine: "mongodb".into(),
        url: format!("mongodb://{authority}{path}{qs}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::PoolConfig;
    use serde_json::json;

    #[test]
    fn database_from_uri_path() {
        assert_eq!(
            database_from_mongo_uri("mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin")
                .as_deref(),
            Some("mystique")
        );
        assert_eq!(
            database_from_mongo_uri("mongodb://127.0.0.1:27017/").as_deref(),
            None
        );
    }

    #[test]
    fn format_prefers_stored_uri() {
        let config = mystique_connector_api::ConnectionConfig {
            engine: "mongodb".into(),
            plugin_config: json!({
                "uri": "mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin"
            }),
            password: None,
            pool: PoolConfig::default(),
        };
        let formatted = format_mongo_url(&config).expect("format");
        assert_eq!(
            formatted.url,
            "mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin"
        );
    }
}
