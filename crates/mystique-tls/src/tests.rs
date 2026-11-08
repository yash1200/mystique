use super::*;
use serde_json::json;

fn cfg(plugin: serde_json::Value) -> serde_json::Value {
    plugin
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
