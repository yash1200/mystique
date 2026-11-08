//! Shared TLS settings for host engines.
//!
//! Plugins must not invent their own `sslMode` mapping. Parse once here, then
//! apply with [`TlsSettings::native_connector`] (Postgres) or the accessors
//! (MySQL / Mongo).
//!
//! No dependency on the plugin SPI: settings are read from the profile's
//! `plugin_config` JSON through [`TlsConfigSource`], and errors are
//! [`TlsError`] (the SPI maps them onto its own error type).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// TLS setup failure. Variants mirror the SPI error kinds they map onto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsError {
    /// Bad mode, missing or unreadable certificate files.
    InvalidConfig(String),
    /// The TLS connector could not be built.
    Connect(String),
}

impl std::fmt::Display for TlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "invalid configuration: {message}"),
            Self::Connect(message) => write!(f, "connection failed: {message}"),
        }
    }
}

impl std::error::Error for TlsError {}

pub type TlsResult<T> = Result<T, TlsError>;

/// Anything that carries the profile's `plugin_config` JSON object.
pub trait TlsConfigSource {
    fn plugin_config(&self) -> &JsonValue;
}

impl TlsConfigSource for JsonValue {
    fn plugin_config(&self) -> &JsonValue {
        self
    }
}

fn str_field(config: &impl TlsConfigSource, key: &str) -> Option<String> {
    config
        .plugin_config()
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

fn bool_field(config: &impl TlsConfigSource, key: &str, default: bool) -> bool {
    config
        .plugin_config()
        .get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

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
    pub fn from_config(config: &impl TlsConfigSource) -> Self {
        if let Some(mode) = str_field(config, "sslMode").or_else(|| str_field(config, "ssl_mode")) {
            let parsed = Self::parse(&mode);
            if parsed == Self::Disable && mode.trim().eq_ignore_ascii_case("disable") {
                // The Require checkbox can be on while the mode string is still
                // `disable`. That must not open a plaintext connection.
                if bool_field(config, "ssl", false) {
                    return Self::Require;
                }
                return Self::Disable;
            }
            if parsed != Self::Disable {
                return parsed;
            }
        }
        if bool_field(config, "ssl", false) {
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
    pub fn from_config(config: &impl TlsConfigSource) -> Self {
        Self {
            mode: SslMode::from_config(config),
            ca_cert_path: path_field(config, "caCertPath")
                .or_else(|| path_field(config, "ca_cert_path")),
            client_cert_path: path_field(config, "clientCertPath")
                .or_else(|| path_field(config, "client_cert_path")),
            client_key_path: path_field(config, "clientKeyPath")
                .or_else(|| path_field(config, "client_key_path")),
            client_key_passphrase: str_field(config, "clientKeyPassphrase")
                .or_else(|| str_field(config, "client_key_passphrase")),
        }
    }

    pub fn enabled(&self) -> bool {
        self.mode.encrypts()
    }

    /// Reject modes this engine cannot do, and require a CA for verify modes.
    ///
    /// Call this before dialing. A missing CA is a validation error, not
    /// "trust any certificate" or "trust the system roots".
    pub fn check(&self, engine: &str) -> TlsResult<()> {
        if let TlsModeSupport::Rejected(message) = tls_mode_support(engine, self.mode) {
            return Err(TlsError::InvalidConfig(message));
        }
        if self.mode.verify_certs() && self.ca_cert_path.is_none() {
            return Err(TlsError::InvalidConfig(format!(
                "{} {} needs a CA certificate file. System roots are not used for this mode.",
                engine_label(engine),
                self.mode.as_str()
            )));
        }
        Ok(())
    }

    /// Combined PEM (cert+key) for drivers that take one file (Mongo).
    pub fn cert_key_file(&self) -> Option<&Path> {
        self.client_key_path
            .as_deref()
            .or(self.client_cert_path.as_deref())
    }

    /// native-tls connector. `None` when SSL is disabled.
    pub fn native_connector(&self) -> TlsResult<Option<native_tls::TlsConnector>> {
        if !self.enabled() {
            return Ok(None);
        }
        self.check("postgres")?;
        let mut builder = native_tls::TlsConnector::builder();
        if !self.mode.verify_certs() {
            builder.danger_accept_invalid_certs(true);
            builder.danger_accept_invalid_hostnames(true);
        } else if !self.mode.verify_hostname() {
            builder.danger_accept_invalid_hostnames(true);
        }
        if let Some(ca) = &self.ca_cert_path {
            // A configured CA is the only trust anchor. System roots stay out.
            builder.disable_built_in_roots(true);
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
            .map_err(|e| TlsError::Connect(format!("TLS init failed: {e}")))
    }
}

fn path_field(config: &impl TlsConfigSource, key: &str) -> Option<PathBuf> {
    str_field(config, key).map(|s| expand_path(&s))
}

fn expand_path(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(raw)
}

fn load_certificate(path: &Path) -> TlsResult<native_tls::Certificate> {
    let bytes = read_file(path)?;
    native_tls::Certificate::from_pem(&bytes)
        .or_else(|_| native_tls::Certificate::from_der(&bytes))
        .map_err(|e| {
            TlsError::InvalidConfig(format!(
                "could not read CA certificate {}: {e}",
                path.display()
            ))
        })
}

fn load_identity(
    cert: Option<&Path>,
    key: Option<&Path>,
    passphrase: Option<&str>,
) -> TlsResult<Option<native_tls::Identity>> {
    match (cert, key) {
        (None, None) => Ok(None),
        (Some(p), None) | (None, Some(p)) if is_pkcs12(p) => {
            let der = read_file(p)?;
            native_tls::Identity::from_pkcs12(&der, passphrase.unwrap_or(""))
                .map(Some)
                .map_err(|e| {
                    TlsError::InvalidConfig(format!(
                        "could not read PKCS#12 identity {}: {e}",
                        p.display()
                    ))
                })
        }
        (Some(cert), Some(key)) => {
            if passphrase.is_some() {
                return Err(TlsError::InvalidConfig(
                    "A key passphrase is only applied to a PKCS#12 client identity. PEM keys must be unencrypted PKCS#8.".into(),
                ));
            }
            let cert_bytes = read_file(cert)?;
            let key_bytes = read_file(key)?;
            native_tls::Identity::from_pkcs8(&cert_bytes, &key_bytes)
                .map(Some)
                .map_err(|e| {
                    TlsError::InvalidConfig(format!(
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
                    TlsError::InvalidConfig(format!(
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

/// One cell of the SSL mode matrix. Every mode is either implemented or
/// rejected with the sentence the user sees. Nothing is left to the driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsModeSupport {
    Implemented,
    Rejected(String),
}

pub fn tls_mode_support(engine: &str, mode: SslMode) -> TlsModeSupport {
    let runtime = runtime_family(engine).unwrap_or("");
    let label = engine_label(engine);
    match (runtime, mode) {
        (_, SslMode::Disable | SslMode::Require) => TlsModeSupport::Implemented,
        ("postgres", SslMode::Prefer | SslMode::VerifyCa | SslMode::VerifyFull) => {
            TlsModeSupport::Implemented
        }
        ("mysql", SslMode::Prefer) => TlsModeSupport::Rejected(format!(
            "{label} cannot try TLS and then fall back to plaintext. Choose require or disable."
        )),
        ("mysql", SslMode::VerifyCa | SslMode::VerifyFull) => TlsModeSupport::Implemented,
        ("mongodb", SslMode::Prefer) => TlsModeSupport::Rejected(format!(
            "{label} cannot try TLS and then fall back to plaintext. Choose require or disable."
        )),
        // The rustls driver always checks the dial hostname and has no switch
        // to verify the CA alone.
        ("mongodb", SslMode::VerifyCa) => TlsModeSupport::Rejected(format!(
            "{label} verify-ca also checks the server hostname. Use verify-full with a CA file, or require for encryption only."
        )),
        ("mongodb", SslMode::VerifyFull) => TlsModeSupport::Implemented,
        _ => TlsModeSupport::Rejected(format!(
            "{label} does not support sslmode={}.",
            mode.as_str()
        )),
    }
}

/// Wire runtime for an engine id or alias: `postgres`, `mysql`, `mongodb` or
/// `sqlite`. Mirrors the SPI product table (`runtime_id`); a test in
/// `mystique-connector-api` fails if the two drift.
pub fn runtime_family(engine: &str) -> Option<&'static str> {
    match engine.trim().to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" | "pg" | "cockroachdb" => Some("postgres"),
        "mysql" | "mariadb" | "tidb" => Some("mysql"),
        "sqlite" => Some("sqlite"),
        "mongodb" | "mongo" | "documentdb" => Some("mongodb"),
        _ => None,
    }
}

fn engine_label(engine: &str) -> String {
    let key = engine.trim().to_ascii_lowercase();
    match key.as_str() {
        "mariadb" => "MariaDB".into(),
        "tidb" => "TiDB".into(),
        "documentdb" => "DocumentDB".into(),
        "mongodb" | "mongo" => "MongoDB".into(),
        "postgres" | "postgresql" | "pg" | "cockroachdb" => "PostgreSQL".into(),
        "mysql" => "MySQL".into(),
        _ => "This database".into(),
    }
}

fn read_file(path: &Path) -> TlsResult<Vec<u8>> {
    fs::read(path)
        .map_err(|e| TlsError::InvalidConfig(format!("could not read {}: {e}", path.display())))
}

#[cfg(test)]
mod tests;
