//! shim: remove after one release. TLS lives in `mystique-tls`; plugins
//! depend on it directly. The SPI only adapts `ConnectionConfig`
//! (`config.rs`) and `TlsError` (`error.rs`).

pub use mystique_tls::{SslMode, TlsSettings};

#[cfg(test)]
mod tests {
    use crate::{ConnectionConfig, ConnectorError, ENGINE_PRODUCTS};

    #[test]
    fn tls_runtime_table_matches_the_product_table() {
        for product in ENGINE_PRODUCTS {
            assert_eq!(
                mystique_tls::runtime_family(product.id),
                Some(product.runtime),
                "{}",
                product.id
            );
        }
        assert_eq!(mystique_tls::runtime_family("oracle"), None);
    }

    #[test]
    fn tls_errors_keep_their_spi_kind_and_text() {
        let config =
            ConnectionConfig::new("postgres", serde_json::json!({ "sslMode": "verify-ca" }));
        let err: ConnectorError = super::TlsSettings::from_config(&config)
            .check("postgres")
            .unwrap_err()
            .into();
        assert!(matches!(err, ConnectorError::InvalidConfig(_)), "{err:?}");
        assert!(
            err.to_string().starts_with("invalid configuration: "),
            "{err}"
        );
    }
}
