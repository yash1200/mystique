use mystique_connector_api::{ConnectorError, ConnectorResult};
use serde_json::Value as JsonValue;

/// Destination the bastion should reach (the real database host).
pub fn ssh_target(plugin_config: &JsonValue) -> ConnectorResult<(String, u16)> {
    if let Some(uri) = plugin_config
        .get("uri")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return host_port_from_uri(uri);
    }
    let host = plugin_config
        .get("host")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(
                "SSH tunnel needs a database host (file databases have no SSH).".into(),
            )
        })?;
    let port = json_port(plugin_config.get("port")).unwrap_or(0);
    if port == 0 {
        return Err(ConnectorError::InvalidConfig(
            "SSH tunnel needs a database port.".into(),
        ));
    }
    Ok((host.to_string(), port))
}

/// Point the plugin at the local forward. Original host stays on the profile.
pub fn rewrite_plugin_config_for_tunnel(
    mut plugin_config: JsonValue,
    local_port: u16,
) -> ConnectorResult<JsonValue> {
    if let Some(uri) = plugin_config
        .get("uri")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let (tls_name, _) = host_port_from_uri(uri)?;
        let rewritten = force_mongo_direct(&rewrite_uri_host_port(uri, "127.0.0.1", local_port)?);
        if let Some(obj) = plugin_config.as_object_mut() {
            obj.insert("uri".into(), JsonValue::String(rewritten));
            remember_tls_name(obj, &tls_name);
        }
        return Ok(plugin_config);
    }
    let obj = plugin_config
        .as_object_mut()
        .ok_or_else(|| ConnectorError::InvalidConfig("plugin_config must be an object".into()))?;
    let tls_name = obj
        .get("host")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    obj.insert("host".into(), JsonValue::String("127.0.0.1".into()));
    obj.insert("port".into(), JsonValue::from(local_port));
    if !tls_name.is_empty() {
        remember_tls_name(obj, &tls_name);
    }
    Ok(plugin_config)
}

/// Host-form Mongo has no URI for [`force_mongo_direct`] to edit.
/// The plugin honors `directConnection` when it dials from host and port.
pub fn pin_forwarded_mongo(engine: &str, mut config: JsonValue) -> JsonValue {
    if mystique_connector_api::runtime_id(engine) != Some("mongodb") {
        return config;
    }
    let uri_is_direct = config
        .get("uri")
        .and_then(|v| v.as_str())
        .is_some_and(|uri| uri.to_ascii_lowercase().contains("directconnection=true"));
    if uri_is_direct {
        return config;
    }
    if let Some(obj) = config.as_object_mut() {
        obj.insert("directConnection".into(), JsonValue::Bool(true));
    }
    config
}

/// The driver must use the local forward, not the replica set's own hosts.
fn force_mongo_direct(uri: &str) -> String {
    if !uri.to_ascii_lowercase().starts_with("mongodb://") {
        return uri.to_string();
    }
    let (before_hash, hash) = match uri.split_once('#') {
        Some((body, hash)) => (body, Some(hash)),
        None => (uri, None),
    };
    let (path, query) = match before_hash.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (before_hash, None),
    };
    let mut params: Vec<String> = query
        .unwrap_or("")
        .split('&')
        .filter(|part| !part.is_empty())
        .filter(|part| {
            let key = part.split_once('=').map(|(key, _)| key).unwrap_or(part);
            !key.eq_ignore_ascii_case("replicaset") && !key.eq_ignore_ascii_case("directconnection")
        })
        .map(str::to_string)
        .collect();
    params.push("directConnection=true".into());
    let mut out = format!("{path}?{}", params.join("&"));
    if let Some(hash) = hash {
        out.push('#');
        out.push_str(hash);
    }
    out
}

fn remember_tls_name(obj: &mut serde_json::Map<String, JsonValue>, host: &str) {
    let existing = obj
        .get("tlsServerName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if existing.is_empty() {
        obj.insert("tlsServerName".into(), JsonValue::String(host.to_string()));
    }
}

fn json_port(v: Option<&JsonValue>) -> Option<u16> {
    match v? {
        JsonValue::Number(n) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
        JsonValue::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn host_port_from_uri(uri: &str) -> ConnectorResult<(String, u16)> {
    let trimmed = uri.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("mongodb+srv://") {
        return Err(ConnectorError::InvalidConfig(
            "SSH tunnel needs mongodb:// with a host, not mongodb+srv.".into(),
        ));
    }
    let rest = trimmed
        .split_once("://")
        .map(|(_, r)| r)
        .ok_or_else(|| ConnectorError::InvalidConfig("connection URI has no host".into()))?;
    let after_at = rest.rsplit_once('@').map(|(_, h)| h).unwrap_or(rest);
    let hostport = after_at
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_at)
        .split(',')
        .next()
        .unwrap_or(after_at)
        .trim();
    let (host, port) = split_host_port(hostport)?;
    let port = if port == 0 {
        if lower.starts_with("mongodb://") {
            27017
        } else if lower.starts_with("mysql://") || lower.starts_with("mariadb://") {
            3306
        } else {
            5432
        }
    } else {
        port
    };
    Ok((host, port))
}

fn split_host_port(hostport: &str) -> ConnectorResult<(String, u16)> {
    if hostport.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "connection URI has no host".into(),
        ));
    }
    if let Some(rest) = hostport.strip_prefix('[') {
        let (host, tail) = rest
            .split_once(']')
            .ok_or_else(|| ConnectorError::InvalidConfig("invalid IPv6 host in URI".into()))?;
        let port = tail
            .strip_prefix(':')
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        return Ok((host.to_string(), port));
    }
    if let Some((host, port)) = hostport.rsplit_once(':') {
        if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            let p: u16 = port.parse().unwrap_or(0);
            return Ok((host.to_string(), p));
        }
    }
    Ok((hostport.to_string(), 0))
}

fn rewrite_uri_host_port(uri: &str, new_host: &str, new_port: u16) -> ConnectorResult<String> {
    let (scheme, rest) = uri
        .split_once("://")
        .ok_or_else(|| ConnectorError::InvalidConfig("connection URI has no host".into()))?;
    let (userinfo, hostpart) = match rest.rsplit_once('@') {
        Some((u, h)) => (Some(u), h),
        None => (None, rest),
    };
    let slash = hostpart.find(['/', '?', '#']).unwrap_or(hostpart.len());
    let tail = hostpart.get(slash..).unwrap_or("");
    let authority = format!("{new_host}:{new_port}");
    Ok(match userinfo {
        Some(u) => format!("{scheme}://{u}@{authority}{tail}"),
        None => format!("{scheme}://{authority}{tail}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn target_from_host_fields() {
        let (h, p) = ssh_target(&json!({"host": "db.internal", "port": 5432})).unwrap();
        assert_eq!((h, p), ("db.internal".into(), 5432));
    }

    #[test]
    fn target_from_mongo_uri() {
        let (h, p) = ssh_target(&json!({
            "uri": "mongodb://user:pass@mongo.internal:27018/app?authSource=admin"
        }))
        .unwrap();
        assert_eq!((h, p), ("mongo.internal".into(), 27018));
    }

    #[test]
    fn srv_uri_is_rejected() {
        let err = ssh_target(&json!({"uri": "mongodb+srv://cluster.example.net/app"})).unwrap_err();
        assert!(err.to_string().contains("mongodb+srv"));
    }

    #[test]
    fn rewrite_host_fields() {
        let out = rewrite_plugin_config_for_tunnel(
            json!({"host": "db.internal", "port": 5432, "database": "app"}),
            5555,
        )
        .unwrap();
        assert_eq!(out["host"], "127.0.0.1");
        assert_eq!(out["port"], 5555);
        assert_eq!(out["tlsServerName"], "db.internal");
        assert_eq!(out["database"], "app");
    }

    #[test]
    fn rewrite_mongo_uri_keeps_userinfo() {
        let out = rewrite_plugin_config_for_tunnel(
            json!({"uri": "mongodb://u:p@mongo.internal:27017/app?authSource=admin"}),
            6000,
        )
        .unwrap();
        assert_eq!(
            out["uri"],
            "mongodb://u:p@127.0.0.1:6000/app?authSource=admin&directConnection=true"
        );
        assert_eq!(out["tlsServerName"], "mongo.internal");
    }

    #[test]
    fn mongo_replica_set_uses_the_forward_only() {
        let out = rewrite_plugin_config_for_tunnel(
            json!({
                "uri": "mongodb://user:pw@mongo1.internal:27017,mongo2.internal:27017/app?replicaSet=rs0&authSource=admin#frag"
            }),
            6001,
        )
        .unwrap();
        assert_eq!(
            out["uri"],
            "mongodb://user:pw@127.0.0.1:6001/app?authSource=admin&directConnection=true#frag"
        );
        assert_eq!(out["tlsServerName"], "mongo1.internal");
    }

    #[test]
    fn postgres_uri_is_not_forced_direct() {
        let out = rewrite_plugin_config_for_tunnel(
            json!({"uri": "postgresql://u:p@db.internal:5432/app?sslmode=require"}),
            6002,
        )
        .unwrap();
        assert_eq!(
            out["uri"],
            "postgresql://u:p@127.0.0.1:6002/app?sslmode=require"
        );
        assert!(pin_forwarded_mongo("postgres", out)
            .get("directConnection")
            .is_none());
    }

    #[test]
    fn mongo_host_fields_are_marked_direct() {
        let out = rewrite_plugin_config_for_tunnel(
            json!({"host": "mongo.internal", "port": 27017, "database": "app"}),
            6004,
        )
        .unwrap();
        let pinned = pin_forwarded_mongo("documentdb", out);
        assert_eq!(pinned["host"], "127.0.0.1");
        assert_eq!(pinned["port"], 6004);
        assert_eq!(pinned["directConnection"], true);
        assert_eq!(pinned["tlsServerName"], "mongo.internal");
        assert_eq!(pinned["database"], "app");
    }

    #[test]
    fn sqlite_path_has_no_target() {
        let err = ssh_target(&json!({"path": "/tmp/app.db"})).unwrap_err();
        assert!(err.to_string().contains("host"));
    }
}
