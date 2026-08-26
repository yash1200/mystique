use mystique_connector_api::{ConnectionConfig, DatabaseConnector, PoolConfig};
use mystique_plugin_mysql::MysqlConnector;
use serde_json::json;

#[tokio::test]
async fn test_connection_refused_returns_err() {
    let config = ConnectionConfig {
        engine: "mysql".into(),
        plugin_config: json!({
            "host": "127.0.0.1",
            "port": 1,
            "database": "mystique",
            "username": "admin"
        }),
        password: Some("123".into()),
        pool: PoolConfig {
            max_size: 1,
            min_idle: 0,
            acquire_timeout_ms: 2000,
            idle_timeout_secs: 10,
        },
    };
    let err = MysqlConnector
        .test_connection(&config)
        .await
        .expect_err("port 1 should refuse");
    let s = err.to_string();
    assert!(
        s.to_lowercase().contains("error")
            || s.to_lowercase().contains("connect")
            || s.to_lowercase().contains("refused")
            || s.to_lowercase().contains("connection"),
        "unexpected error shape: {s}"
    );
}

#[tokio::test]
async fn test_connection_against_local_docker() {
    let config = ConnectionConfig {
        engine: "mysql".into(),
        plugin_config: json!({
            "host": "127.0.0.1",
            "port": 3306,
            "database": "mystique",
            "username": "admin"
        }),
        password: Some("123".into()),
        pool: PoolConfig {
            max_size: 2,
            min_idle: 0,
            acquire_timeout_ms: 5000,
            idle_timeout_secs: 10,
        },
    };
    let result = MysqlConnector
        .test_connection(&config)
        .await
        .expect("connect to local mystique-mysql");
    assert!(result.ok, "msg={}", result.message);
    let ver = result.server_version.unwrap_or_default();
    assert!(
        ver.to_lowercase().contains("mysql") || ver.starts_with('8') || !ver.is_empty(),
        "version={ver}"
    );
}
