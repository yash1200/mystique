use super::*;
use serde_json::json;

#[test]
fn parse_connection_url_postgres_and_mysql() {
    let mgr = ConnectionManager::bootstrap();
    let pg = mgr
        .parse_connection_url(
            "postgres",
            "postgresql://app:s3cret@db.example.com:5433/appdb?sslmode=require",
        )
        .expect("postgres url");
    assert_eq!(pg.plugin_config["host"], "db.example.com");
    assert_eq!(pg.plugin_config["port"], 5433);
    assert_eq!(pg.plugin_config["database"], "appdb");
    assert_eq!(pg.password.as_deref(), Some("s3cret"));
    assert_eq!(pg.plugin_config["ssl"], true);

    let my = mgr
        .parse_connection_url("mysql", "mysql://admin:123@127.0.0.1:3307/mystique")
        .expect("mysql url");
    assert_eq!(my.plugin_config["host"], "127.0.0.1");
    assert_eq!(my.plugin_config["port"], 3307);
    assert_eq!(my.plugin_config["username"], "admin");
    assert_eq!(my.password.as_deref(), Some("123"));

    // MariaDB alias → mysql plugin
    let md = mgr
        .parse_connection_url("mariadb", "mariadb://root@localhost/db")
        .expect("mariadb url");
    assert_eq!(md.engine, "mysql");
    assert_eq!(md.plugin_config["database"], "db");
}

#[test]
fn format_connection_url_postgres_and_mysql() {
    let mgr = ConnectionManager::bootstrap();
    let pg = mgr
        .format_connection_url(
            "postgres",
            json!({
                "host": "db.example.com",
                "port": 5433,
                "database": "appdb",
                "username": "app",
                "ssl": true,
                "sslMode": "require"
            }),
            Some("s3cret".into()),
        )
        .expect("format postgres");
    assert!(
        pg.url.contains("postgresql://") && pg.url.contains("db.example.com:5433"),
        "url={}",
        pg.url
    );
    assert!(pg.url.contains("sslmode=require"), "url={}", pg.url);

    let my = mgr
        .format_connection_url(
            "mysql",
            json!({
                "host": "127.0.0.1",
                "port": 3306,
                "database": "mystique",
                "username": "admin"
            }),
            Some("123".into()),
        )
        .expect("format mysql");
    assert!(my.url.starts_with("mysql://"), "url={}", my.url);
    assert!(my.url.contains("admin"), "url={}", my.url);

    let mongo = mgr
        .format_connection_url(
            "mongodb",
            json!({ "uri": "mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin" }),
            None,
        )
        .expect("format mongo");
    assert!(mongo.url.starts_with("mongodb://"), "url={}", mongo.url);

    let sqlite = mgr
        .format_connection_url("sqlite", json!({ "path": "/tmp/app.sqlite" }), None)
        .expect("format sqlite");
    assert_eq!(sqlite.url, "file:///tmp/app.sqlite");
}

#[tokio::test]
async fn test_unknown_engine_returns_structured_fail() {
    let mgr = ConnectionManager::bootstrap();
    let result = mgr
        .test_connection(&OpenSessionRequest {
            engine: "nope".into(),
            plugin_config: json!({}),
            password: None,
            pool: None,
            session_id: None,
            profile_id: None,
        })
        .await
        .unwrap();
    assert!(!result.ok);
    assert!(result.message.contains("Unknown engine"));
}

#[tokio::test]
async fn test_postgres_missing_db_structured_fail() {
    let mgr = ConnectionManager::bootstrap();
    let result = mgr
        .test_connection(&OpenSessionRequest {
            engine: "postgres".into(),
            plugin_config: json!({ "host": "127.0.0.1", "port": 5432 }),
            password: None,
            pool: Some(PoolConfig {
                max_size: 1,
                min_idle: 0,
                acquire_timeout_ms: 1500,
                idle_timeout_secs: 5,
            }),
            session_id: None,
            profile_id: None,
        })
        .await
        .unwrap();
    assert!(!result.ok, "expected fail");
    println!("msg={}", result.message);
    assert!(
        result.message.contains("database") || result.message.contains("missing"),
        "msg={}",
        result.message
    );
}

#[tokio::test]
async fn test_postgres_unreachable_structured_fail() {
    let mgr = ConnectionManager::bootstrap();
    let result = mgr
        .test_connection(&OpenSessionRequest {
            engine: "postgres".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 1,
                "database": "postgres",
                "username": "postgres"
            }),
            password: Some("x".into()),
            pool: Some(PoolConfig {
                max_size: 1,
                min_idle: 0,
                acquire_timeout_ms: 1500,
                idle_timeout_secs: 5,
            }),
            session_id: None,
            profile_id: None,
        })
        .await
        .unwrap();
    assert!(!result.ok);
    println!("unreachable msg={}", result.message);
    assert!(!result.message.is_empty());
    // Prefer root-cause wording over opaque wrappers
    assert!(
        result.message.to_lowercase().contains("refused")
            || result.message.to_lowercase().contains("connect")
            || result.message.to_lowercase().contains("error"),
        "msg={}",
        result.message
    );
}

#[tokio::test]
async fn test_postgres_local_success_when_available() {
    let user = std::env::var("USER").unwrap_or_else(|_| "postgres".into());
    let mgr = ConnectionManager::bootstrap();
    let result = mgr
        .test_connection(&OpenSessionRequest {
            engine: "postgres".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 5432,
                "database": "postgres",
                "username": user,
            }),
            password: None,
            pool: Some(PoolConfig {
                max_size: 1,
                min_idle: 0,
                acquire_timeout_ms: 3000,
                idle_timeout_secs: 5,
            }),
            session_id: None,
            profile_id: None,
        })
        .await
        .unwrap();
    println!(
        "local success? ok={} msg={} version={:?}",
        result.ok, result.message, result.server_version
    );
    // Skip-friendly: if Postgres isn't running, just assert structured fail.
    if result.ok {
        assert!(result
            .server_version
            .as_deref()
            .unwrap_or("")
            .contains("PostgreSQL"));
    } else {
        assert!(!result.message.is_empty());
    }
}
