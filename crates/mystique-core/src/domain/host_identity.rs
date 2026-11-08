//! Which plugin_config fields name the server a saved secret belongs to.
//!
//! Host identity is `host` + port, the hosts inside `uri`, or the SQLite
//! file `path`, plus `tlsServerName` (Postgres dials that name when `host`
//! is not an IP). Database name, SSL mode, and username are not identity:
//! the client may override those. A different host does not reuse the
//! saved secret.

use std::collections::BTreeSet;

use serde_json::Value as JsonValue;

/// True when both configs dial the same server (or the same SQLite file).
pub fn same_connection_target(engine: &str, stored: &JsonValue, client: &JsonValue) -> bool {
    target_of(engine, stored) == target_of(engine, client)
        && tls_server_name(stored) == tls_server_name(client)
}

/// Stored config, plus client fields that do not change which server we dial.
///
/// `password` and `username` stay on the stored side. Those are the
/// credential, re-injected from the keychain at connect.
pub fn overlay_non_host(stored: &JsonValue, client: &JsonValue) -> JsonValue {
    let mut out = stored.clone();
    let (Some(base), Some(over)) = (out.as_object_mut(), client.as_object()) else {
        return stored.clone();
    };
    for (key, value) in over {
        if is_identity_key(key) {
            continue;
        }
        base.insert(key.clone(), value.clone());
    }
    out
}

fn is_identity_key(key: &str) -> bool {
    matches!(
        key,
        "host"
            | "port"
            | "uri"
            | "path"
            | "file"
            | "password"
            | "username"
            | "user"
            | "tlsServerName"
    )
}

fn tls_server_name(config: &JsonValue) -> Option<String> {
    text(config, "tlsServerName").map(|name| name.to_ascii_lowercase())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    Empty,
    File(String),
    Hosts(BTreeSet<(String, u16)>),
}

fn target_of(engine: &str, config: &JsonValue) -> Target {
    let runtime = mystique_connector_api::runtime_id(engine).unwrap_or("");
    if runtime == "sqlite" {
        let path = text(config, "path")
            .or_else(|| text(config, "file"))
            .unwrap_or_default();
        return if path.is_empty() {
            Target::Empty
        } else {
            Target::File(path)
        };
    }
    if let Some(uri) = text(config, "uri") {
        if let Some(hosts) = uri_hosts(engine, &uri) {
            return Target::Hosts(hosts);
        }
    }
    let Some(host) = text(config, "host").filter(|h| !h.is_empty()) else {
        return Target::Empty;
    };
    let port = json_port(config.get("port")).unwrap_or_else(|| default_port(engine, ""));
    Target::Hosts(BTreeSet::from([(host.to_ascii_lowercase(), port)]))
}

fn text(config: &JsonValue, key: &str) -> Option<String> {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn uri_hosts(engine: &str, uri: &str) -> Option<BTreeSet<(String, u16)>> {
    let (scheme, rest) = uri.trim().split_once("://")?;
    let scheme_key = scheme.to_ascii_lowercase();
    let after_user = rest.rsplit_once('@').map(|(_, host)| host).unwrap_or(rest);
    let authority = after_user
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .trim();
    if authority.is_empty() {
        return None;
    }
    let mut hosts = BTreeSet::new();
    for part in split_authority(authority) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (host, port) = split_host_port(part);
        if host.is_empty() {
            continue;
        }
        let port = if port == 0 {
            default_port(engine, &scheme_key)
        } else {
            port
        };
        hosts.insert((host.to_ascii_lowercase(), port));
    }
    if hosts.is_empty() {
        None
    } else {
        Some(hosts)
    }
}

fn split_authority(authority: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    for (i, ch) in authority.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&authority[start..i]);
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&authority[start..]);
    out
}

fn split_host_port(hostport: &str) -> (String, u16) {
    if let Some(rest) = hostport.strip_prefix('[') {
        let Some((host, tail)) = rest.split_once(']') else {
            return (hostport.to_string(), 0);
        };
        let port = tail
            .strip_prefix(':')
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        return (host.to_string(), port);
    }
    if let Some((host, port)) = hostport.rsplit_once(':') {
        if !host.is_empty() && !host.contains(':') && port.chars().all(|c| c.is_ascii_digit()) {
            return (host.to_string(), port.parse().unwrap_or(0));
        }
    }
    (hostport.to_string(), 0)
}

fn json_port(value: Option<&JsonValue>) -> Option<u16> {
    match value? {
        JsonValue::Number(n) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
        JsonValue::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn default_port(engine: &str, scheme: &str) -> u16 {
    match scheme {
        "mongodb+srv" => 0,
        "mongodb" => 27017,
        "postgres" | "postgresql" => 5432,
        "mysql" => 3306,
        _ => mystique_connector_api::product(engine)
            .map(|p| p.default_port)
            .unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_port_matches_the_engine_default() {
        let stored = json!({"host": "db.internal", "database": "app"});
        let client = json!({"host": "DB.internal", "port": 5432, "database": "other"});
        assert!(same_connection_target("postgres", &stored, &client));
        let merged = overlay_non_host(&stored, &client);
        assert_eq!(merged["host"], "db.internal");
        assert_eq!(merged["database"], "other");
        assert!(merged.get("port").is_none());
    }

    #[test]
    fn different_host_is_a_different_target() {
        let stored = json!({"host": "10.0.0.1", "port": 5432});
        let client = json!({"host": "10.0.0.2", "port": 5432});
        assert!(!same_connection_target("postgres", &stored, &client));
    }

    #[test]
    fn mongo_uri_ignores_userinfo_and_database() {
        let stored = json!({"uri": "mongodb://user:secret@127.0.0.1:27017/app?authSource=admin"});
        let client = json!({"uri": "mongodb://127.0.0.1/other"});
        assert!(same_connection_target("documentdb", &stored, &client));
    }

    #[test]
    fn tls_server_name_is_part_of_the_target() {
        let stored = json!({"host": "db.internal", "port": 5432});
        let client = json!({"host": "db.internal", "port": 5432, "tlsServerName": "evil.example"});
        assert!(!same_connection_target("postgres", &stored, &client));
        let named = json!({"host": "10.0.0.5", "tlsServerName": "DB.internal"});
        assert!(same_connection_target(
            "postgres",
            &named,
            &json!({"host": "10.0.0.5", "tlsServerName": "db.internal", "database": "x"})
        ));
        let merged = overlay_non_host(&stored, &client);
        assert!(merged.get("tlsServerName").is_none());
    }

    #[test]
    fn sqlite_path_is_the_target() {
        let stored = json!({"path": "/tmp/a.db"});
        let client = json!({"path": "/tmp/b.db"});
        assert!(!same_connection_target("sqlite", &stored, &client));
        assert!(same_connection_target(
            "sqlite",
            &stored,
            &json!({"path": "/tmp/a.db", "password": "nope"})
        ));
    }
}
