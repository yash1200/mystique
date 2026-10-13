use std::time::Duration;

use mongodb::options::{ClientOptions, ServerAddress, Tls, TlsOptions};
use mongodb::Client;
use mystique_connector_api::{ConnectionConfig, ConnectorError, ConnectorResult, TlsSettings};

use super::url::database_from_mongo_uri;

/// Build a Mongo client and resolve the default application database.
///
/// Default DB resolution order:
/// 1. explicit `pluginConfig.database`
/// 2. path segment in the connection URI (`…/mystique?…`)
/// 3. `None` (caller / explorer must pick a database)
pub(super) async fn build_client(
    config: &ConnectionConfig,
) -> ConnectorResult<(Client, Option<String>)> {
    let mut options = if let Some(uri) = config.str_field("uri") {
        ClientOptions::parse(&uri)
            .await
            .map_err(|e| ConnectorError::InvalidConfig(e.to_string()))?
    } else {
        let host = config
            .str_field("host")
            .unwrap_or_else(|| "127.0.0.1".into());
        let port = config.u16_field("port", 27017);
        let mut opts = ClientOptions::default();
        opts.hosts = vec![ServerAddress::Tcp {
            host,
            port: Some(port),
        }];
        opts
    };
    apply_direct_connection(config, &mut options);
    apply_retry_writes(config, &mut options);

    // Pool sizing (driver-managed)
    options.max_pool_size = Some(config.pool.max_size);
    options.min_pool_size = Some(config.pool.min_idle.min(config.pool.max_size));
    options.connect_timeout = Some(Duration::from_millis(config.pool.acquire_timeout_ms));
    if config.pool.idle_timeout_secs > 0 {
        options.max_idle_time = Some(Duration::from_secs(config.pool.idle_timeout_secs));
    }

    // Fields fill a URI that no longer carries userinfo. Keep authSource the
    // URI already parsed (or an explicit field) instead of replacing it.
    let user = config
        .str_field("username")
        .or_else(|| config.str_field("user"));
    let password = config
        .password
        .clone()
        .or_else(|| config.str_field("password"));
    if let (Some(username), Some(password)) = (user, password) {
        let mut cred = options.credential.clone().unwrap_or_default();
        cred.username = Some(username);
        cred.password = Some(password);
        if cred.source.as_deref().unwrap_or("").is_empty() {
            cred.source = Some(
                config
                    .str_field("authSource")
                    .or_else(|| config.str_field("auth_source"))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "admin".into()),
            );
        }
        options.credential = Some(cred);
    }

    options.app_name = Some("Mystique".into());

    let tls = TlsSettings::from_config(config);
    tls.check(&config.engine)?;
    if tls.mode == mystique_connector_api::SslMode::VerifyFull {
        if let Some(message) = mongo_verify_full_tunnel_error(config) {
            return Err(ConnectorError::InvalidConfig(message));
        }
    }
    if tls.enabled() {
        let mut tls_opts = TlsOptions::default();
        tls_opts.allow_invalid_certificates = Some(!tls.mode.verify_certs());
        tls_opts.ca_file_path = tls.ca_cert_path.clone();
        tls_opts.cert_key_file_path = mongo_identity_path(&tls)?;
        if let Some(passphrase) = tls.client_key_passphrase.clone() {
            tls_opts.tls_certificate_key_file_password = Some(passphrase.into_bytes());
        }
        options.tls = Some(Tls::Enabled(tls_opts));
    }

    let default_db = config
        .str_field("database")
        .filter(|s| !s.is_empty())
        .or_else(|| options.default_database.clone())
        .or_else(|| {
            config
                .str_field("uri")
                .and_then(|u| database_from_mongo_uri(&u))
        });

    let client =
        Client::with_options(options).map_err(|e| ConnectorError::Connect(e.to_string()))?;
    Ok((client, default_db))
}

fn apply_direct_connection(config: &ConnectionConfig, options: &mut ClientOptions) {
    if config.bool_field("directConnection", false) {
        options.direct_connection = Some(true);
    }
}

/// DocumentDB connect stamps `retryWrites: false`. MongoDB omits the key and
/// keeps the driver default.
fn apply_retry_writes(config: &ConnectionConfig, options: &mut ClientOptions) {
    if config
        .plugin_config
        .get("retryWrites")
        .and_then(|v| v.as_bool())
        == Some(false)
    {
        options.retry_writes = Some(false);
    }
}

fn mongo_verify_full_tunnel_error(config: &ConnectionConfig) -> Option<String> {
    let name = config.str_field("tlsServerName")?;
    let dial = mongo_dial_host(config).unwrap_or_default();
    if is_loopback(&dial) && !dial.eq_ignore_ascii_case(&name) {
        Some(
            "MongoDB verify-full through an SSH tunnel cannot check the real server name. The driver dials 127.0.0.1 and has no separate TLS name. Use require, or connect without SSH."
                .into(),
        )
    } else {
        None
    }
}

fn mongo_dial_host(config: &ConnectionConfig) -> Option<String> {
    if let Some(uri) = config.str_field("uri") {
        let rest = uri
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(uri.as_str());
        let hostpart = rest.rsplit_once('@').map(|(_, host)| host).unwrap_or(rest);
        let authority = hostpart
            .split(['/', '?', '#'])
            .next()
            .unwrap_or(hostpart)
            .trim();
        let host = authority
            .rsplit_once(':')
            .map(|(host, _)| host)
            .unwrap_or(authority);
        if !host.is_empty() {
            return Some(host.to_string());
        }
    }
    config.str_field("host")
}

fn is_loopback(host: &str) -> bool {
    matches!(
        host.trim()
            .trim_matches(['[', ']'])
            .to_ascii_lowercase()
            .as_str(),
        "127.0.0.1" | "localhost" | "::1"
    )
}

/// Mongo wants one PEM that contains the certificate and the private key.
fn mongo_identity_path(tls: &TlsSettings) -> ConnectorResult<Option<std::path::PathBuf>> {
    match (tls.client_cert_path.as_ref(), tls.client_key_path.as_ref()) {
        (None, None) => Ok(None),
        (Some(path), None) | (None, Some(path)) => {
            require_combined_pem(path)?;
            Ok(Some(path.clone()))
        }
        (Some(cert), Some(key)) if cert == key => {
            require_combined_pem(cert)?;
            Ok(Some(cert.clone()))
        }
        (Some(cert), Some(key)) => {
            require_combined_pem_parts(cert, key)?;
            Ok(Some(write_combined_pem(cert, key)?))
        }
    }
}

fn require_combined_pem(path: &std::path::Path) -> ConnectorResult<()> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        ConnectorError::InvalidConfig(format!("could not read {}: {e}", path.display()))
    })?;
    if text.contains("CERTIFICATE") && text.contains("PRIVATE KEY") {
        Ok(())
    } else {
        Err(ConnectorError::InvalidConfig(
            "MongoDB client identity must be one PEM file that contains the certificate and the private key.".into(),
        ))
    }
}

fn require_combined_pem_parts(
    cert: &std::path::Path,
    key: &std::path::Path,
) -> ConnectorResult<()> {
    let cert_text = std::fs::read_to_string(cert).map_err(|e| {
        ConnectorError::InvalidConfig(format!("could not read {}: {e}", cert.display()))
    })?;
    let key_text = std::fs::read_to_string(key).map_err(|e| {
        ConnectorError::InvalidConfig(format!("could not read {}: {e}", key.display()))
    })?;
    if !cert_text.contains("CERTIFICATE") || !key_text.contains("PRIVATE KEY") {
        return Err(ConnectorError::InvalidConfig(
            "MongoDB client identity must be PEM. PKCS#12 is not accepted.".into(),
        ));
    }
    Ok(())
}

fn write_combined_pem(
    cert: &std::path::Path,
    key: &std::path::Path,
) -> ConnectorResult<std::path::PathBuf> {
    let cert_text = std::fs::read_to_string(cert).map_err(|e| {
        ConnectorError::InvalidConfig(format!("could not read {}: {e}", cert.display()))
    })?;
    let key_text = std::fs::read_to_string(key).map_err(|e| {
        ConnectorError::InvalidConfig(format!("could not read {}: {e}", key.display()))
    })?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "mystique-mongo-identity-{}-{nanos}.pem",
        std::process::id()
    ));
    std::fs::write(&path, format!("{cert_text}\n{key_text}")).map_err(|e| {
        ConnectorError::InvalidConfig(format!("could not write MongoDB client identity: {e}"))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::ConnectionConfig;
    use serde_json::json;

    #[tokio::test]
    async fn verify_ca_is_rejected() {
        let err = build_client(&ConnectionConfig::new(
            "documentdb",
            json!({
                "uri": "mongodb://127.0.0.1:27017/app",
                "sslMode": "verify-ca",
                "caCertPath": "/tmp/ca.pem"
            }),
        ))
        .await
        .unwrap_err();
        assert!(err.to_string().contains("verify-ca"), "{err}");
        assert!(err.to_string().contains("DocumentDB"), "{err}");
    }

    #[tokio::test]
    async fn verify_full_through_ssh_is_rejected() {
        let err = build_client(&ConnectionConfig::new(
            "mongodb",
            json!({
                "uri": "mongodb://127.0.0.1:27017/app",
                "sslMode": "verify-full",
                "caCertPath": "/tmp/ca.pem",
                "tlsServerName": "mongo.internal"
            }),
        ))
        .await
        .unwrap_err();
        assert!(err.to_string().contains("SSH tunnel"), "{err}");
    }

    #[test]
    fn documentdb_forces_retry_writes_off() {
        let mut options = ClientOptions::default();
        options.retry_writes = Some(true);
        apply_retry_writes(
            &ConnectionConfig::new(
                "mongodb",
                json!({"uri": "mongodb://127.0.0.1/app?retryWrites=true", "retryWrites": false}),
            ),
            &mut options,
        );
        assert_eq!(options.retry_writes, Some(false));
    }

    #[test]
    fn mongodb_leaves_driver_retry_writes() {
        let mut options = ClientOptions::default();
        options.retry_writes = Some(true);
        apply_retry_writes(
            &ConnectionConfig::new("mongodb", json!({"uri": "mongodb://127.0.0.1/app"})),
            &mut options,
        );
        assert_eq!(options.retry_writes, Some(true));
    }

    #[test]
    fn direct_connection_flag_pins_host_form() {
        let mut options = ClientOptions::default();
        apply_direct_connection(
            &ConnectionConfig::new(
                "mongodb",
                json!({"host": "127.0.0.1", "port": 27017, "directConnection": true}),
            ),
            &mut options,
        );
        assert_eq!(options.direct_connection, Some(true));

        let mut plain = ClientOptions::default();
        apply_direct_connection(
            &ConnectionConfig::new("mongodb", json!({"host": "127.0.0.1"})),
            &mut plain,
        );
        assert_eq!(plain.direct_connection, None);
    }

    #[tokio::test]
    async fn prefer_is_rejected() {
        let err = build_client(&ConnectionConfig::new(
            "mongodb",
            json!({
                "uri": "mongodb://127.0.0.1:27017/app",
                "sslMode": "prefer"
            }),
        ))
        .await
        .unwrap_err();
        assert!(err.to_string().contains("cannot try TLS"), "{err}");
    }
}
