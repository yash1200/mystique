use mystique_connector_api::{
    ConnectionConfig, DatabaseConnector, PoolConfig, QueryMode, ValidatedQuery,
};
use mystique_plugin_postgres::PostgresConnector;
use serde_json::json;

#[tokio::test]
async fn test_connection_refused_returns_err() {
    let config = ConnectionConfig {
        engine: "postgres".into(),
        plugin_config: json!({
            "host": "127.0.0.1",
            "port": 1,
            "database": "postgres",
            "username": "postgres"
        }),
        password: Some("postgres".into()),
        pool: PoolConfig {
            max_size: 1,
            min_idle: 0,
            acquire_timeout_ms: 2000,
            idle_timeout_secs: 10,
        },
    };
    let err = PostgresConnector
        .test_connection(&config)
        .await
        .expect_err("port 1 should refuse");
    let s = err.to_string();
    assert!(
        s.contains("error")
            || s.contains("connect")
            || s.contains("refused")
            || s.contains("Connection"),
        "unexpected error shape: {s}"
    );
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn test_connection_against_local_homebrew() {
    let user = std::env::var("USER").unwrap_or_else(|_| "yashjohri".into());
    let config = ConnectionConfig {
        engine: "postgres".into(),
        plugin_config: json!({
            "host": "127.0.0.1",
            "port": 5432,
            "database": "postgres",
            "username": user
        }),
        password: None,
        pool: PoolConfig {
            max_size: 1,
            min_idle: 0,
            acquire_timeout_ms: 5000,
            idle_timeout_secs: 10,
        },
    };
    let result = PostgresConnector
        .test_connection(&config)
        .await
        .expect("connect");
    assert!(result.ok, "msg={}", result.message);
    assert!(
        result
            .server_version
            .as_deref()
            .unwrap_or("")
            .contains("PostgreSQL"),
        "version={:?}",
        result.server_version
    );
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn test_connection_default_user_postgres_has_clear_error() {
    let config = ConnectionConfig {
        engine: "postgres".into(),
        plugin_config: json!({
            "host": "127.0.0.1",
            "port": 5432,
            "database": "postgres"
        }),
        password: None,
        pool: PoolConfig {
            max_size: 1,
            min_idle: 0,
            acquire_timeout_ms: 5000,
            idle_timeout_secs: 10,
        },
    };
    let err = PostgresConnector
        .test_connection(&config)
        .await
        .expect_err("role postgres should not exist on homebrew");
    let s = err.to_string();
    assert!(
        s.to_lowercase().contains("postgres")
            && (s.to_lowercase().contains("role")
                || s.to_lowercase().contains("exist")
                || s.to_lowercase().contains("authentication")
                || s.to_lowercase().contains("password")),
        "error should mention the real postgres failure, got: {s}"
    );
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn bad_sql_surfaces_postgres_message() {
    let user = std::env::var("USER").unwrap_or_else(|_| "yashjohri".into());
    let config = ConnectionConfig {
        engine: "postgres".into(),
        plugin_config: json!({
            "host": "127.0.0.1",
            "port": 5432,
            "database": "postgres",
            "username": user
        }),
        password: None,
        pool: PoolConfig {
            max_size: 1,
            min_idle: 0,
            acquire_timeout_ms: 5000,
            idle_timeout_secs: 10,
        },
    };
    let session = PostgresConnector.connect(&config).await.expect("connect");
    let err = session
        .execute_query(&ValidatedQuery {
            text: "SELEC 1".into(),
            mode: QueryMode::Read,
            max_rows: 10,
            timeout_ms: 5_000,
            collection: None,
            database: None,
        })
        .await
        .expect_err("invalid SQL should fail");
    let s = err.to_string();
    assert!(
        !s.to_ascii_lowercase().contains("db error"),
        "must not hide the server message behind 'db error': {s}"
    );
    assert!(
        s.to_ascii_lowercase().contains("syntax") || s.to_ascii_lowercase().contains("selec"),
        "expected a descriptive Postgres syntax error, got: {s}"
    );
}
