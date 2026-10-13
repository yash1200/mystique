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
            if parsed == Self::Disable && mode.trim().eq_ignore_ascii_case("disable") {
                // The Require checkbox can be on while the mode string is still
                // `disable`. That must not open a plaintext connection.
                if config.bool_field("ssl", false) {
                    return Self::Require;
                }
                return Self::Disable;
            }
            if parsed != Self::Disable {
                return parsed;
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

    /// Reject modes this engine cannot do, and require a CA for verify modes.
    ///
    /// Call this before dialing. A missing CA is a validation error, not
    /// "trust any certificate" or "trust the system roots".
    pub fn check(&self, engine: &str) -> ConnectorResult<()> {
        if let TlsModeSupport::Rejected(message) = tls_mode_support(engine, self.mode) {
            return Err(ConnectorError::InvalidConfig(message));
        }
        if self.mode.verify_certs() && self.ca_cert_path.is_none() {
            return Err(ConnectorError::InvalidConfig(format!(
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
    pub fn native_connector(&self) -> ConnectorResult<Option<native_tls::TlsConnector>> {
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
            if passphrase.is_some() {
                return Err(ConnectorError::InvalidConfig(
                    "A key passphrase is only applied to a PKCS#12 client identity. PEM keys must be unencrypted PKCS#8.".into(),
                ));
            }
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

/// One cell of the SSL mode matrix. Every mode is either implemented or
/// rejected with the sentence the user sees. Nothing is left to the driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsModeSupport {
    Implemented,
    Rejected(String),
}

pub fn tls_mode_support(engine: &str, mode: SslMode) -> TlsModeSupport {
    let runtime = crate::runtime_id(engine).unwrap_or("");
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
    fn ssl_true_with_disable_is_require() {
        let s = TlsSettings::from_config(&cfg(json!({"ssl": true, "sslMode": "disable"})));
        assert_eq!(s.mode, SslMode::Require);
        assert!(s.enabled());
    }

    #[test]
    fn explicit_disable_without_ssl_flag_stays_plaintext() {
        let s = TlsSettings::from_config(&cfg(json!({"ssl": false, "sslMode": "disable"})));
        assert_eq!(s.mode, SslMode::Disable);
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
    fn verify_ca_without_a_ca_file_fails_before_dial() {
        let s = TlsSettings::from_config(&cfg(json!({"sslMode": "verify-ca"})));
        let err = s.check("postgres").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("CA certificate"), "{msg}");
        assert!(msg.contains("verify-ca"), "{msg}");
    }

    #[test]
    fn verify_full_without_a_ca_file_fails_before_dial() {
        let s = TlsSettings::from_config(&cfg(json!({"sslMode": "verify-full"})));
        let err = s.check("mysql").unwrap_err();
        assert!(err.to_string().contains("CA certificate"), "{err}");
    }

    #[test]
    fn ssl_mode_matrix_names_every_cell() {
        use SslMode::*;
        let cases = [
            ("postgres", Disable, TlsModeSupport::Implemented),
            ("postgres", Prefer, TlsModeSupport::Implemented),
            ("postgres", Require, TlsModeSupport::Implemented),
            ("postgres", VerifyCa, TlsModeSupport::Implemented),
            ("cockroachdb", VerifyFull, TlsModeSupport::Implemented),
            (
                "mysql",
                Prefer,
                TlsModeSupport::Rejected(
                    "MySQL cannot try TLS and then fall back to plaintext. Choose require or disable."
                        .into(),
                ),
            ),
            ("mysql", Require, TlsModeSupport::Implemented),
            ("mysql", VerifyCa, TlsModeSupport::Implemented),
            ("mysql", VerifyFull, TlsModeSupport::Implemented),
            (
                "mariadb",
                Prefer,
                TlsModeSupport::Rejected(
                    "MariaDB cannot try TLS and then fall back to plaintext. Choose require or disable."
                        .into(),
                ),
            ),
            ("tidb", Disable, TlsModeSupport::Implemented),
            ("tidb", VerifyFull, TlsModeSupport::Implemented),
            (
                "mongodb",
                Prefer,
                TlsModeSupport::Rejected(
                    "MongoDB cannot try TLS and then fall back to plaintext. Choose require or disable."
                        .into(),
                ),
            ),
            (
                "mongodb",
                VerifyCa,
                TlsModeSupport::Rejected(
                    "MongoDB verify-ca also checks the server hostname. Use verify-full with a CA file, or require for encryption only."
                        .into(),
                ),
            ),
            ("mongodb", VerifyFull, TlsModeSupport::Implemented),
            ("documentdb", Require, TlsModeSupport::Implemented),
            (
                "documentdb",
                VerifyCa,
                TlsModeSupport::Rejected(
                    "DocumentDB verify-ca also checks the server hostname. Use verify-full with a CA file, or require for encryption only."
                        .into(),
                ),
            ),
        ];
        for (engine, mode, expected) in cases {
            assert_eq!(
                tls_mode_support(engine, mode),
                expected,
                "{engine} {}",
                mode.as_str()
            );
        }
    }

    #[test]
    fn verify_ca_trusts_only_the_configured_ca() {
        let dir = std::env::temp_dir().join(format!(
            "mystique-tls-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let ca = dir.join("ca.pem");
        let ca_key = dir.join("ca.key");
        let other = dir.join("other-ca.pem");
        let other_key = dir.join("other-ca.key");
        let server_key = dir.join("server.key");
        let csr = dir.join("server.csr");
        let server_pem = dir.join("server.pem");
        let p12 = dir.join("server.p12");
        let ext = dir.join("ext.cnf");
        std::fs::write(
            &ext,
            "subjectAltName=DNS:localhost\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nbasicConstraints=CA:FALSE\n",
        )
        .unwrap();
        openssl(&[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            ca_key.to_str().unwrap(),
            "-out",
            ca.to_str().unwrap(),
            "-days",
            "1",
            "-subj",
            "/CN=Mystique Test CA",
        ]);
        openssl(&[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            other_key.to_str().unwrap(),
            "-out",
            other.to_str().unwrap(),
            "-days",
            "1",
            "-subj",
            "/CN=Other CA",
        ]);
        openssl(&["genrsa", "-out", server_key.to_str().unwrap(), "2048"]);
        openssl(&[
            "req",
            "-new",
            "-key",
            server_key.to_str().unwrap(),
            "-out",
            csr.to_str().unwrap(),
            "-subj",
            "/CN=localhost",
        ]);
        openssl(&[
            "x509",
            "-req",
            "-in",
            csr.to_str().unwrap(),
            "-CA",
            ca.to_str().unwrap(),
            "-CAkey",
            ca_key.to_str().unwrap(),
            "-CAcreateserial",
            "-out",
            server_pem.to_str().unwrap(),
            "-days",
            "1",
            "-extfile",
            ext.to_str().unwrap(),
        ]);
        openssl(&[
            "pkcs12",
            "-export",
            "-inkey",
            server_key.to_str().unwrap(),
            "-in",
            server_pem.to_str().unwrap(),
            "-passout",
            "pass:changeit",
            "-out",
            p12.to_str().unwrap(),
        ]);
        let identity = native_tls::Identity::from_pkcs12(&std::fs::read(&p12).unwrap(), "changeit")
            .expect("server identity");
        let acceptor = native_tls::TlsAcceptor::new(identity).expect("acceptor");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let serve = std::sync::Arc::new(acceptor);
        let background = std::sync::Arc::clone(&serve);
        let _thread = std::thread::spawn(move || {
            for _ in 0..2 {
                let Ok((sock, _)) = listener.accept() else {
                    break;
                };
                let _ = background.accept(sock);
            }
        });

        let wrong = TlsSettings::from_config(&cfg(json!({
            "sslMode": "verify-ca",
            "caCertPath": other
        })));
        let wrong_connector = wrong.native_connector().unwrap().expect("connector");
        let wrong_stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let rejected = wrong_connector.connect("localhost", wrong_stream);
        assert!(
            rejected.is_err(),
            "a certificate from another CA must not verify"
        );

        let right = TlsSettings::from_config(&cfg(json!({
            "sslMode": "verify-ca",
            "caCertPath": ca
        })));
        let right_connector = right.native_connector().unwrap().expect("connector");
        let right_stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        right_connector
            .connect("localhost", right_stream)
            .expect("the configured CA verifies its own server");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn openssl(args: &[&str]) {
        let status = std::process::Command::new("openssl")
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("openssl is required for the private CA fixture");
        assert!(status.success(), "openssl {args:?} failed");
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
