//! Shared TLS settings for host engines.
//!
//! Plugins must not invent their own `sslMode` mapping. Parse once here, then
//! apply with [`TlsSettings::native_connector`] (Postgres) or the accessors
//! (MySQL / Mongo).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ConnectorError, ConnectorResult};
use crate::ConnectionConfig;

/// libpq-style SSL mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SslMode {
    #[default]
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl SslMode {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "prefer" => Self::Prefer,
            "require" => Self::Require,
            "verify-ca" | "verifyca" => Self::VerifyCa,
            "verify-full" | "verifyfull" => Self::VerifyFull,
            _ => Self::Disable,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Prefer => "prefer",
            Self::Require => "require",
            Self::VerifyCa => "verify-ca",
            Self::VerifyFull => "verify-full",
        }
    }

    pub fn encrypts(self) -> bool {
        !matches!(self, Self::Disable)
    }

    pub fn verify_certs(self) -> bool {
        matches!(self, Self::VerifyCa | Self::VerifyFull)
    }

    pub fn verify_hostname(self) -> bool {
        matches!(self, Self::VerifyFull)
    }

    /// From `ssl` / `sslMode` on plugin_config. `ssl: true` with no mode → require.
    pub fn from_config(config: &ConnectionConfig) -> Self {
        if let Some(mode) = config
            .str_field("sslMode")
            .or_else(|| config.str_field("ssl_mode"))
        {
            let parsed = Self::parse(&mode);
            if parsed != Self::Disable {
                return parsed;
            }
            // Explicit disable wins even if `ssl: true`.
            if mode.trim().eq_ignore_ascii_case("disable") {
                return Self::Disable;
            }
        }
        if config.bool_field("ssl", false) {
            Self::Require
        } else {
            Self::Disable
        }
    }
}

/// CA + optional client identity, plus the chosen [`SslMode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsSettings {
    pub mode: SslMode,
    pub ca_cert_path: Option<PathBuf>,
    pub client_cert_path: Option<PathBuf>,
    pub client_key_path: Option<PathBuf>,
    pub client_key_passphrase: Option<String>,
}

impl TlsSettings {
    pub fn from_config(config: &ConnectionConfig) -> Self {
        Self {
            mode: SslMode::from_config(config),
            ca_cert_path: path_field(config, "caCertPath")
                .or_else(|| path_field(config, "ca_cert_path")),
            client_cert_path: path_field(config, "clientCertPath")
                .or_else(|| path_field(config, "client_cert_path")),
            client_key_path: path_field(config, "clientKeyPath")
                .or_else(|| path_field(config, "client_key_path")),
            client_key_passphrase: config
                .str_field("clientKeyPassphrase")
                .or_else(|| config.str_field("client_key_passphrase")),
        }
    }

    pub fn enabled(&self) -> bool {
        self.mode.encrypts()
    }

    /// Combined PEM (cert+key) for drivers that take one file (Mongo).
    pub fn cert_key_file(&self) -> Option<&Path> {
        self.client_key_path
            .as_deref()
            .or(self.client_cert_path.as_deref())
    }

    /// native-tls connector. `None` when SSL is disabled.
    pub fn native_connector(&self) -> ConnectorResult<Option<native_tls::TlsConnector>> {
        if !self.enabled() {
            return Ok(None);
        }
        let mut builder = native_tls::TlsConnector::builder();
        if !self.mode.verify_certs() {
            builder.danger_accept_invalid_certs(true);
            builder.danger_accept_invalid_hostnames(true);
        } else if !self.mode.verify_hostname() {
            builder.danger_accept_invalid_hostnames(true);
        }
        if let Some(ca) = &self.ca_cert_path {
            builder.add_root_certificate(load_certificate(ca)?);
        }
        if let Some(identity) = load_identity(
            self.client_cert_path.as_deref(),
            self.client_key_path.as_deref(),
            self.client_key_passphrase.as_deref(),
        )? {
            builder.identity(identity);
        }
        builder
            .build()
            .map(Some)
            .map_err(|e| ConnectorError::Connect(format!("TLS init failed: {e}")))
    }
}

fn path_field(config: &ConnectionConfig, key: &str) -> Option<PathBuf> {
    config.str_field(key).map(|s| expand_path(&s))
}

fn expand_path(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(raw)
}

fn load_certificate(path: &Path) -> ConnectorResult<native_tls::Certificate> {
    let bytes = read_file(path)?;
    native_tls::Certificate::from_pem(&bytes)
        .or_else(|_| native_tls::Certificate::from_der(&bytes))
        .map_err(|e| {
            ConnectorError::InvalidConfig(format!(
                "could not read CA certificate {}: {e}",
                path.display()
            ))
        })
}

fn load_identity(
    cert: Option<&Path>,
    key: Option<&Path>,
    passphrase: Option<&str>,
) -> ConnectorResult<Option<native_tls::Identity>> {
    match (cert, key) {
        (None, None) => Ok(None),
        (Some(p), None) | (None, Some(p)) if is_pkcs12(p) => {
            let der = read_file(p)?;
            native_tls::Identity::from_pkcs12(&der, passphrase.unwrap_or(""))
                .map(Some)
                .map_err(|e| {
                    ConnectorError::InvalidConfig(format!(
                        "could not read PKCS#12 identity {}: {e}",
                        p.display()
                    ))
                })
        }
        (Some(cert), Some(key)) => {
            let cert_bytes = read_file(cert)?;
            let key_bytes = read_file(key)?;
            native_tls::Identity::from_pkcs8(&cert_bytes, &key_bytes)
                .map(Some)
                .map_err(|e| {
                    ConnectorError::InvalidConfig(format!(
                        "could not read client certificate {} / key {}: {e}",
                        cert.display(),
                        key.display()
                    ))
                })
        }
        (Some(p), None) | (None, Some(p)) => {
            let bytes = read_file(p)?;
            native_tls::Identity::from_pkcs8(&bytes, &bytes)
                .map(Some)
                .map_err(|e| {
                    ConnectorError::InvalidConfig(format!(
                        "could not read client identity {}: {e}",
                        p.display()
                    ))
                })
        }
    }
}

fn is_pkcs12(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()),
        Some(ext) if ext == "p12" || ext == "pfx"
    )
}

fn read_file(path: &Path) -> ConnectorResult<Vec<u8>> {
    fs::read(path).map_err(|e| {
        ConnectorError::InvalidConfig(format!("could not read {}: {e}", path.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(plugin: serde_json::Value) -> ConnectionConfig {
        ConnectionConfig::new("postgres", plugin)
    }

    #[test]
    fn ssl_true_without_mode_is_require() {
        let s = TlsSettings::from_config(&cfg(json!({"ssl": true})));
        assert_eq!(s.mode, SslMode::Require);
        assert!(s.enabled());
    }

    #[test]
    fn explicit_disable_wins_over_ssl_true() {
        let s = TlsSettings::from_config(&cfg(json!({"ssl": true, "sslMode": "disable"})));
        assert_eq!(s.mode, SslMode::Disable);
        assert!(!s.enabled());
    }

    #[test]
    fn verify_full_from_ssl_mode() {
        let s = TlsSettings::from_config(&cfg(json!({"sslMode": "verify-full"})));
        assert_eq!(s.mode, SslMode::VerifyFull);
        assert!(s.mode.verify_certs());
        assert!(s.mode.verify_hostname());
    }

    #[test]
    fn native_connector_none_when_disabled() {
        let s = TlsSettings::from_config(&cfg(json!({"ssl": false})));
        assert!(s.native_connector().unwrap().is_none());
    }

    #[test]
    fn native_connector_require_builds_without_files() {
        let s = TlsSettings::from_config(&cfg(json!({"sslMode": "require"})));
        assert!(s.native_connector().unwrap().is_some());
    }

    #[test]
    fn missing_ca_file_errors() {
        let s = TlsSettings::from_config(&cfg(json!({
            "sslMode": "verify-full",
            "caCertPath": "/no/such/ca.pem"
        })));
        let err = s.native_connector().unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("could not read"),
            "{err}"
        );
    }
}
