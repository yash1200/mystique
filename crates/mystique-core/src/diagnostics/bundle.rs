//! The diagnostics bundle: a `.tar.gz` a user attaches to a bug report.
//!
//! In it, under `mystique-diagnostics/`:
//! - `README.txt`: what is in the bundle and what was left out;
//! - `manifest.json`: app version, OS, arch, app-database schema version
//!   (applied and expected), enabled engine plugins, vault source and state;
//! - `settings.json`: app settings, minus any key that names a secret
//!   (token, password, API key, …); values go through [`redact_secrets`];
//! - `connections.json`: one row per saved profile with the engine,
//!   environment, read-only flag, whether a password is saved, SSH on and its
//!   auth kind, TLS on, and the host class (`local`, `remote`, `file` or
//!   `unset`). No name, notes, host, port, user, database, path or URI;
//! - `logs/`: the recent log files, each line through [`redact_secrets`].
//!
//! Never in it: passwords, keys, tokens, the vault key, saved queries, query
//! history, SQL text, chat history, hosts or user names.

use std::path::Path;

use serde_json::{json, Value as JsonValue};

use crate::application::AppServices;
use crate::diagnostics::{log_files, LOG_FILE_NAME};
use crate::domain::{now_iso, ConnectionProfile};
use crate::error::AppResult;
use crate::redact::{is_secret_key, redact_json, redact_secrets};
use crate::storage::sqlite::SCHEMA_VERSION;

/// Folder every entry sits under in the archive.
pub const BUNDLE_ROOT: &str = "mystique-diagnostics";

/// What the shell knows that core does not.
pub struct BundleContext<'a> {
    pub app_version: &'a str,
    /// App log directory. `None` leaves `logs/` out.
    pub log_dir: Option<&'a Path>,
}

const README: &str = "\
Mystique diagnostics bundle

manifest.json     app version, OS, CPU architecture, app database schema
                  version, enabled engine plugins, vault source and state.
settings.json     app settings. Keys that name a secret (token, password,
                  API key) are left out; passwords in values are masked.
connections.json  one row per saved connection: engine, environment,
                  read-only, password saved, SSH on, TLS on, and whether the
                  host is local or remote. No names, hosts, users, databases.
logs/             recent app logs. Passwords in URIs and key=value text are
                  masked. Logs never hold SQL text or parameter values.

Not included: passwords, keys, tokens, saved queries, query history, AI chats.
";

/// `mystique-diagnostics-20261109-143005.tar.gz`, in local time.
pub fn default_bundle_name() -> String {
    chrono::Local::now()
        .format("mystique-diagnostics-%Y%m%d-%H%M%S.tar.gz")
        .to_string()
}

/// Build the bundle as `.tar.gz` bytes.
pub fn build_bundle(app: &AppServices, ctx: &BundleContext<'_>) -> AppResult<Vec<u8>> {
    let mut entries: Vec<(String, Vec<u8>)> = vec![
        ("README.txt".into(), README.as_bytes().to_vec()),
        ("manifest.json".into(), pretty(&manifest(app, ctx))?),
        ("settings.json".into(), pretty(&settings(app)?)?),
        ("connections.json".into(), pretty(&connections(app)?)?),
    ];
    if let Some(dir) = ctx.log_dir {
        for path in log_files(dir, LOG_FILE_NAME) {
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| LOG_FILE_NAME.into());
            let text = String::from_utf8_lossy(&bytes);
            let mut redacted = String::with_capacity(text.len());
            for line in text.lines() {
                redacted.push_str(&redact_secrets(line));
                redacted.push('\n');
            }
            entries.push((format!("logs/{name}"), redacted.into_bytes()));
        }
    }
    archive(&entries)
}

fn pretty(value: &JsonValue) -> AppResult<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn manifest(app: &AppServices, ctx: &BundleContext<'_>) -> JsonValue {
    let plugins: Vec<JsonValue> = app
        .connections()
        .list_engines()
        .into_iter()
        .map(|e| json!({ "id": e.id, "name": e.display_name, "available": e.available }))
        .collect();
    let vault = app.vault().status();
    json!({
        "appVersion": ctx.app_version,
        "os": std::env::consts::OS,
        "osFamily": std::env::consts::FAMILY,
        "arch": std::env::consts::ARCH,
        "debugBuild": cfg!(debug_assertions),
        "generatedAt": now_iso(),
        "schemaVersion": app.db.schema_version().ok(),
        "schemaVersionExpected": SCHEMA_VERSION,
        "plugins": plugins,
        "vault": {
            "source": vault.source,
            "state": vault.state,
            "message": vault.message.as_deref().map(redact_secrets),
        },
    })
}

fn settings(app: &AppServices) -> AppResult<JsonValue> {
    let rows: Vec<JsonValue> = app
        .settings()
        .list()?
        .into_iter()
        .filter(|s| !is_secret_key(&s.key))
        .map(|s| {
            let value = match serde_json::from_str::<JsonValue>(&s.value) {
                Ok(parsed @ (JsonValue::Object(_) | JsonValue::Array(_))) => {
                    redact_json(&parsed).to_string()
                }
                _ => redact_secrets(&s.value),
            };
            json!({ "key": s.key, "value": value, "updatedAt": s.updated_at })
        })
        .collect();
    Ok(JsonValue::Array(rows))
}

fn connections(app: &AppServices) -> AppResult<JsonValue> {
    Ok(JsonValue::Array(
        app.profiles().list()?.iter().map(profile_summary).collect(),
    ))
}

/// The parts of a profile that help debugging and identify nothing.
fn profile_summary(p: &ConnectionProfile) -> JsonValue {
    json!({
        "engine": p.engine,
        "environment": p.environment,
        "readOnly": p.read_only,
        "passwordSaved": p.password_set,
        "savePassword": p.save_password,
        "ssh": p.ssh.enabled,
        "sshAuth": p.ssh.enabled.then_some(p.ssh.auth),
        "tls": tls_on(&p.plugin_config),
        "host": host_class(&p.plugin_config),
        "queryTimeoutSecs": p.query_timeout_secs,
    })
}

fn tls_on(config: &JsonValue) -> bool {
    let flag = |key: &str| config.get(key).and_then(JsonValue::as_bool) == Some(true);
    let mode_on = config
        .get("sslMode")
        .and_then(JsonValue::as_str)
        .map(|m| !m.trim().is_empty() && !m.eq_ignore_ascii_case("disable"))
        .unwrap_or(false);
    flag("ssl") || flag("tls") || mode_on
}

fn host_class(config: &JsonValue) -> &'static str {
    let text = |key: &str| {
        config
            .get(key)
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let host = text("host").or_else(|| {
        ["uri", "url", "connectionString"]
            .into_iter()
            .find_map(text)
            .map(uri_host)
    });
    match host {
        Some(h) if is_local_host(h) => "local",
        Some(_) => "remote",
        None if text("path").is_some() => "file",
        None => "unset",
    }
}

/// The first host of a URI, without user info or port.
fn uri_host(uri: &str) -> &str {
    let rest = uri.split_once("://").map(|(_, r)| r).unwrap_or(uri);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let hosts = authority
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(authority);
    let first = hosts.split(',').next().unwrap_or(hosts);
    if let Some(v6) = first.strip_prefix('[') {
        return v6.split(']').next().unwrap_or(v6);
    }
    first.split(':').next().unwrap_or(first)
}

fn is_local_host(host: &str) -> bool {
    let h = host.trim().trim_start_matches('[').trim_end_matches(']');
    h.eq_ignore_ascii_case("localhost")
        || h == "::1"
        || h.starts_with("127.")
        || h.starts_with('/')
        || h.eq_ignore_ascii_case("host.docker.internal")
}

fn archive(entries: &[(String, Vec<u8>)]) -> AppResult<Vec<u8>> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);
    let mtime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    for (name, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(mtime);
        header.set_cksum();
        tar.append_data(
            &mut header,
            format!("{BUNDLE_ROOT}/{name}"),
            bytes.as_slice(),
        )?;
    }
    Ok(tar.into_inner()?.finish()?)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;
    use crate::connectors::ConnectionManager;
    use crate::domain::{SaveConnectionInput, SshConfig};
    use crate::secrets::SecretStore;
    use crate::storage::AppDatabase;

    fn app() -> AppServices {
        AppServices::with_secrets(
            AppDatabase::open_in_memory().unwrap(),
            ConnectionManager::bootstrap(),
            SecretStore::memory(),
        )
    }

    fn unpack(bytes: &[u8]) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            let mut text = String::new();
            entry.read_to_string(&mut text).unwrap();
            out.push((path, text));
        }
        out
    }

    #[test]
    fn the_bundle_holds_no_secret_host_or_user() {
        let app = app();
        app.profiles()
            .save(SaveConnectionInput {
                id: None,
                name: "Billing prod".into(),
                engine: "postgres".into(),
                color_id: "teal".into(),
                environment: "production".into(),
                read_only: true,
                notes: Some("owner: alice".into()),
                plugin_config: json!({
                    "host": "billing.db.internal",
                    "port": 5432,
                    "username": "billing_admin",
                    "database": "billing",
                    "ssl": true,
                    "sslMode": "verify-full"
                }),
                password: Some("hunter2-secret".into()),
                secret_ref: None,
                clear_secret: false,
                copy_secret_from: None,
                favorite: None,
                ssh_config: SshConfig {
                    enabled: true,
                    host: "bastion.internal".into(),
                    user: "jump".into(),
                    ..SshConfig::default()
                },
                ssh_password: Some("ssh-secret".into()),
                clear_ssh_secret: false,
                tls_key_passphrase: None,
                query_timeout_secs: None,
                save_password: None,
            })
            .unwrap();
        app.settings().set("mcp.token", "tok-secret").unwrap();
        app.settings().set("theme", "dark").unwrap();
        app.settings()
            .set("last.url", "postgres://u:url-secret@h/db")
            .unwrap();

        let logs =
            std::env::temp_dir().join(format!("mystique-bundle-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(
            logs.join(LOG_FILE_NAME),
            "INFO session opened engine=postgres\nWARN dsn password=log-secret host=h\n",
        )
        .unwrap();

        let bytes = build_bundle(
            &app,
            &BundleContext {
                app_version: "9.9.9",
                log_dir: Some(&logs),
            },
        )
        .unwrap();
        std::fs::remove_dir_all(&logs).unwrap();

        let files = unpack(&bytes);
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "mystique-diagnostics/README.txt",
                "mystique-diagnostics/manifest.json",
                "mystique-diagnostics/settings.json",
                "mystique-diagnostics/connections.json",
                "mystique-diagnostics/logs/mystique.log",
            ]
        );
        let all: String = files.iter().map(|(_, t)| t.as_str()).collect();
        for secret in [
            "hunter2-secret",
            "ssh-secret",
            "tok-secret",
            "url-secret",
            "log-secret",
            "billing.db.internal",
            "bastion.internal",
            "billing_admin",
            "Billing prod",
            "alice",
        ] {
            assert!(!all.contains(secret), "{secret} leaked");
        }

        let manifest: JsonValue = serde_json::from_str(&files[1].1).unwrap();
        assert_eq!(manifest["appVersion"], "9.9.9");
        assert_eq!(manifest["schemaVersion"], SCHEMA_VERSION);
        assert!(manifest["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == "postgres"));

        let settings: JsonValue = serde_json::from_str(&files[2].1).unwrap();
        let keys: Vec<&str> = settings
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s["key"].as_str())
            .collect();
        assert!(keys.contains(&"theme"));
        assert!(!keys.contains(&"mcp.token"));

        let connections: JsonValue = serde_json::from_str(&files[3].1).unwrap();
        assert_eq!(
            connections,
            json!([{
                "engine": "postgres",
                "environment": "production",
                "readOnly": true,
                "passwordSaved": true,
                "savePassword": true,
                "ssh": true,
                "sshAuth": "password",
                "tls": true,
                "host": "remote",
                "queryTimeoutSecs": null,
            }])
        );
        assert!(files[4].1.contains("password=***"));
    }

    #[test]
    fn host_class_covers_hosts_uris_and_files() {
        assert_eq!(host_class(&json!({ "host": "localhost" })), "local");
        assert_eq!(host_class(&json!({ "host": "127.0.0.1" })), "local");
        assert_eq!(host_class(&json!({ "host": "db.example.com" })), "remote");
        assert_eq!(
            host_class(&json!({ "uri": "mongodb://u:p@127.0.0.1:27017,h2/db" })),
            "local"
        );
        assert_eq!(
            host_class(&json!({ "uri": "mongodb+srv://u:p@cluster0.x.net/" })),
            "remote"
        );
        assert_eq!(host_class(&json!({ "path": "/tmp/a.sqlite" })), "file");
        assert_eq!(host_class(&json!({})), "unset");
    }

    #[test]
    fn tls_follows_ssl_tls_and_ssl_mode() {
        assert!(tls_on(&json!({ "ssl": true })));
        assert!(tls_on(&json!({ "tls": true })));
        assert!(tls_on(&json!({ "sslMode": "require" })));
        assert!(!tls_on(&json!({ "sslMode": "disable" })));
        assert!(!tls_on(&json!({})));
    }
}
