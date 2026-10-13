use mystique_connector_api::{
    ConnectorResult, ConnectorSession, EngineCapabilities, PoolConfig, PoolStatus,
};
use serde_json::json;

use super::{ConnectionManager, LiveSession, OpenSessionRequest};

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

    // Product ids resolve to the runtime plugin (P1.1).
    let md = mgr
        .parse_connection_url("mariadb", "mariadb://root@localhost/db")
        .expect("mariadb url");
    assert_eq!(md.engine, "mysql");
    assert_eq!(md.plugin_config["database"], "db");

    let cr = mgr
        .parse_connection_url("cockroachdb", "postgresql://root@localhost:26257/defaultdb")
        .expect("cockroach url");
    assert_eq!(cr.plugin_config["port"], 26257);
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
            ssh: None,
            tls_key_passphrase: None,
            environment: None,
            connection_name: None,
            profile_read_only: false,
            use_saved_ssh: true,
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
            ssh: None,
            tls_key_passphrase: None,
            environment: None,
            connection_name: None,
            profile_read_only: false,
            use_saved_ssh: true,
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
            ssh: None,
            tls_key_passphrase: None,
            environment: None,
            connection_name: None,
            profile_read_only: false,
            use_saved_ssh: true,
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
            ssh: None,
            tls_key_passphrase: None,
            environment: None,
            connection_name: None,
            profile_read_only: false,
            use_saved_ssh: true,
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

#[tokio::test]
async fn close_then_open_another_session_does_not_hang() {
    let dir = std::env::temp_dir();
    let a = dir.join("mystique-close-a.sqlite");
    let b = dir.join("mystique-close-b.sqlite");
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
    let mgr = ConnectionManager::bootstrap();
    let open = |path: &std::path::Path, id: &str| OpenSessionRequest {
        engine: "sqlite".into(),
        plugin_config: json!({ "path": path.to_string_lossy() }),
        password: None,
        pool: Some(PoolConfig {
            max_size: 1,
            min_idle: 0,
            acquire_timeout_ms: 3000,
            idle_timeout_secs: 5,
        }),
        session_id: Some(id.into()),
        profile_id: None,
        ssh: None,
        tls_key_passphrase: None,
        environment: None,
        connection_name: None,
        profile_read_only: false,
        use_saved_ssh: true,
    };
    mgr.open_session(open(&a, "sess-a")).await.expect("open a");
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        mgr.close_session("sess-a"),
    )
    .await
    .expect("close a timed out")
    .expect("close a");
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        mgr.open_session(open(&b, "sess-b")),
    )
    .await
    .expect("open b timed out")
    .expect("open b");
    let _ = mgr.close_session("sess-b").await;
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
}

#[tokio::test]
async fn production_write_requires_unlock() {
    let path = std::env::temp_dir().join("mystique-guard.sqlite");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(OpenSessionRequest {
        engine: "sqlite".into(),
        plugin_config: json!({ "path": path.to_string_lossy() }),
        password: None,
        pool: None,
        session_id: Some("prod".into()),
        profile_id: None,
        ssh: None,
        tls_key_passphrase: None,
        environment: Some("production".into()),
        connection_name: Some("Prod DB".into()),
        profile_read_only: false,
        use_saved_ssh: true,
    })
    .await
    .expect("open");

    let err = mgr
        .execute_query(mystique_connector_api::ExecuteQueryRequest {
            session_id: "prod".into(),
            text: "CREATE TABLE t (id INTEGER)".into(),
            mode: mystique_connector_api::QueryMode::Write,
            max_rows: None,
            timeout_ms: None,
            profile_read_only: false,
            confirm_object_name: None,
            collection: None,
            database: None,
        })
        .await
        .expect_err("blocked");
    let msg = err.to_string();
    assert!(msg.contains("writeGuard"), "{msg}");
    assert!(msg.contains("unlock"), "{msg}");

    mgr.unlock_writes("prod").await.expect("unlock");
    mgr.execute_query(mystique_connector_api::ExecuteQueryRequest {
        session_id: "prod".into(),
        text: "CREATE TABLE t (id INTEGER)".into(),
        mode: mystique_connector_api::QueryMode::Write,
        max_rows: None,
        timeout_ms: None,
        profile_read_only: false,
        confirm_object_name: None,
        collection: None,
        database: None,
    })
    .await
    .expect("write after unlock");

    let drop_err = mgr
        .execute_query(mystique_connector_api::ExecuteQueryRequest {
            session_id: "prod".into(),
            text: "DROP TABLE t".into(),
            mode: mystique_connector_api::QueryMode::Write,
            max_rows: None,
            timeout_ms: None,
            profile_read_only: false,
            confirm_object_name: None,
            collection: None,
            database: None,
        })
        .await
        .expect_err("drop needs name");
    let drop_msg = drop_err.to_string();
    assert!(drop_msg.contains("typeConfirm"), "{drop_msg}");

    mgr.execute_query(mystique_connector_api::ExecuteQueryRequest {
        session_id: "prod".into(),
        text: "DROP TABLE t".into(),
        mode: mystique_connector_api::QueryMode::Write,
        max_rows: None,
        timeout_ms: None,
        profile_read_only: false,
        confirm_object_name: Some("t".into()),
        collection: None,
        database: None,
    })
    .await
    .expect("drop after type confirm");

    let _ = mgr.close_session("prod").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn local_write_skips_unlock() {
    let path = std::env::temp_dir().join("mystique-guard-local.sqlite");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(OpenSessionRequest {
        engine: "sqlite".into(),
        plugin_config: json!({ "path": path.to_string_lossy() }),
        password: None,
        pool: None,
        session_id: Some("local".into()),
        profile_id: None,
        ssh: None,
        tls_key_passphrase: None,
        environment: Some("local".into()),
        connection_name: Some("Local".into()),
        profile_read_only: false,
        use_saved_ssh: true,
    })
    .await
    .expect("open");
    mgr.execute_query(mystique_connector_api::ExecuteQueryRequest {
        session_id: "local".into(),
        text: "CREATE TABLE t (id INTEGER)".into(),
        mode: mystique_connector_api::QueryMode::Write,
        max_rows: None,
        timeout_ms: None,
        profile_read_only: false,
        confirm_object_name: None,
        collection: None,
        database: None,
    })
    .await
    .expect("local write");
    let _ = mgr.close_session("local").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn live_read_only_ignores_a_client_write_flag() {
    let path = std::env::temp_dir().join(format!("mystique-live-ro-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    let path_str = path.to_string_lossy().to_string();

    mgr.open_session(OpenSessionRequest {
        engine: "sqlite".into(),
        plugin_config: json!({ "path": path_str }),
        password: None,
        pool: None,
        session_id: Some("seed".into()),
        profile_id: None,
        ssh: None,
        tls_key_passphrase: None,
        environment: Some("local".into()),
        connection_name: None,
        profile_read_only: false,
        use_saved_ssh: true,
    })
    .await
    .expect("open seed");
    mgr.execute_query(write_req(
        "seed",
        "CREATE TABLE t (id INTEGER); INSERT INTO t VALUES (1)",
    ))
    .await
    .expect("seed");
    mgr.close_session("seed").await.expect("close seed");

    mgr.open_session(OpenSessionRequest {
        engine: "sqlite".into(),
        plugin_config: json!({ "path": path_str }),
        password: None,
        pool: None,
        session_id: Some("ro".into()),
        profile_id: None,
        ssh: None,
        tls_key_passphrase: None,
        environment: Some("local".into()),
        connection_name: None,
        profile_read_only: true,
        use_saved_ssh: true,
    })
    .await
    .expect("open read-only");

    let err = mgr
        .execute_query(mystique_connector_api::ExecuteQueryRequest {
            session_id: "ro".into(),
            text: "DELETE FROM t".into(),
            mode: mystique_connector_api::QueryMode::Write,
            max_rows: None,
            timeout_ms: None,
            profile_read_only: false,
            confirm_object_name: None,
            collection: None,
            database: None,
        })
        .await
        .expect_err("client write flag must not override the session");
    assert!(
        err.to_string().to_ascii_lowercase().contains("read-only")
            || err.to_string().to_ascii_lowercase().contains("readonly"),
        "{err}"
    );

    let count = mgr
        .execute_query(mystique_connector_api::ExecuteQueryRequest {
            session_id: "ro".into(),
            text: "SELECT COUNT(*) FROM t".into(),
            mode: mystique_connector_api::QueryMode::Write,
            max_rows: None,
            timeout_ms: None,
            profile_read_only: false,
            confirm_object_name: None,
            collection: None,
            database: None,
        })
        .await
        .expect("count");
    assert_eq!(count.rows[0][0], json!(1));

    let _ = mgr.close_session("ro").await;
    let _ = std::fs::remove_file(&path);
}

fn sqlite_open(
    path: &std::path::Path,
    session_id: &str,
    environment: &str,
    read_only: bool,
) -> OpenSessionRequest {
    OpenSessionRequest {
        engine: "sqlite".into(),
        plugin_config: json!({ "path": path.to_string_lossy() }),
        password: None,
        pool: None,
        session_id: Some(session_id.into()),
        profile_id: None,
        ssh: None,
        tls_key_passphrase: None,
        environment: Some(environment.into()),
        connection_name: Some(session_id.into()),
        profile_read_only: read_only,
        use_saved_ssh: true,
    }
}

fn temp_db(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "mystique-{label}-{}-{nanos}.sqlite",
        std::process::id()
    ))
}

fn import_req(
    session_id: &str,
    path: &str,
    read_only: bool,
) -> mystique_connector_api::ImportObjectRequest {
    mystique_connector_api::ImportObjectRequest {
        session_id: session_id.into(),
        name: "t".into(),
        schema: None,
        database: None,
        kind: None,
        path: path.into(),
        format: mystique_connector_api::ImportFormat::Csv,
        encoding: mystique_connector_api::ImportEncoding::Utf8,
        mapping: Vec::new(),
        on_duplicate: mystique_connector_api::ImportOnDuplicate::UpdateOnDuplicate,
        profile_read_only: read_only,
    }
}

async fn count_rows(mgr: &ConnectionManager, session_id: &str) -> i64 {
    let count = mgr
        .execute_query(mystique_connector_api::ExecuteQueryRequest {
            session_id: session_id.into(),
            text: "SELECT COUNT(*) FROM t".into(),
            mode: mystique_connector_api::QueryMode::Read,
            max_rows: None,
            timeout_ms: None,
            profile_read_only: false,
            confirm_object_name: None,
            collection: None,
            database: None,
        })
        .await
        .expect("count");
    count.rows[0][0].as_i64().expect("count cell")
}

#[tokio::test]
async fn import_into_production_requires_unlock_before_apply() {
    let db_path = temp_db("import-prod");
    let csv_path = db_path.with_extension("csv");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&csv_path);
    std::fs::write(&csv_path, "id\n7\n").expect("csv");

    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&db_path, "seed", "local", false))
        .await
        .expect("open seed");
    mgr.execute_query(write_req("seed", "CREATE TABLE t (id INTEGER PRIMARY KEY)"))
        .await
        .expect("create");
    mgr.close_session("seed").await.expect("close seed");

    mgr.open_session(sqlite_open(&db_path, "prod", "production", false))
        .await
        .expect("open prod");
    let err = mgr
        .import_object(import_req("prod", &csv_path.to_string_lossy(), false))
        .await
        .expect_err("production import prompts");
    let msg = err.to_string();
    assert!(msg.contains("writeGuard"), "{msg}");
    assert!(msg.contains("unlock"), "{msg}");
    assert_eq!(count_rows(&mgr, "prod").await, 0);

    mgr.unlock_writes("prod").await.expect("unlock");
    mgr.import_object(import_req("prod", &csv_path.to_string_lossy(), false))
        .await
        .expect("import after unlock");
    assert_eq!(count_rows(&mgr, "prod").await, 1);

    let _ = mgr.close_session("prod").await;
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&csv_path);
}

#[tokio::test]
async fn import_into_read_only_session_ignores_client_flag() {
    let db_path = temp_db("import-ro");
    let csv_path = db_path.with_extension("csv");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&csv_path);
    std::fs::write(&csv_path, "id\n7\n").expect("csv");

    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&db_path, "seed", "local", false))
        .await
        .expect("open seed");
    mgr.execute_query(write_req(
        "seed",
        "CREATE TABLE t (id INTEGER); INSERT INTO t VALUES (1)",
    ))
    .await
    .expect("seed");
    mgr.close_session("seed").await.expect("close seed");

    mgr.open_session(sqlite_open(&db_path, "ro", "local", true))
        .await
        .expect("open read-only");
    let err = mgr
        .import_object(import_req("ro", &csv_path.to_string_lossy(), false))
        .await
        .expect_err("client flag must not override the session");
    assert!(
        err.to_string().to_ascii_lowercase().contains("read-only"),
        "{err}"
    );
    assert_eq!(count_rows(&mgr, "ro").await, 1);

    let _ = mgr.close_session("ro").await;
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&csv_path);
}

struct GuardProbe {
    ran: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait::async_trait]
impl ConnectorSession for GuardProbe {
    fn engine_id(&self) -> &str {
        "mongodb"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::mongodb_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        Ok(())
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        None
    }

    async fn close(&self) -> ConnectorResult<()> {
        Ok(())
    }

    async fn execute_query(
        &self,
        query: &mystique_connector_api::ValidatedQuery,
    ) -> ConnectorResult<mystique_connector_api::QueryResult> {
        self.ran.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(mystique_connector_api::QueryResult {
            engine: "mongodb".into(),
            mode: query.mode,
            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
            duration_ms: 0,
            message: Some(query.text.clone()),
        })
    }
}

async fn insert_probe(
    mgr: &ConnectionManager,
    session_id: &str,
    read_only: bool,
) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let session: std::sync::Arc<dyn ConnectorSession> = std::sync::Arc::new(GuardProbe {
        ran: std::sync::Arc::clone(&ran),
    });
    mgr.sessions.write().await.insert(
        session_id.into(),
        LiveSession {
            session,
            display_name: "Mongo".into(),
            opened_at: String::new(),
            product_id: "mongodb".into(),
            _tunnel: None,
            environment: "production".into(),
            read_only,
            connection_name: "Prod Mongo".into(),
        },
    );
    ran
}

#[tokio::test]
async fn mongo_delete_many_on_production_prompts() {
    let mgr = ConnectionManager::bootstrap();
    let ran = insert_probe(&mgr, "prod-mongo", false).await;
    let err = mgr
        .execute_query(mystique_connector_api::ExecuteQueryRequest {
            session_id: "prod-mongo".into(),
            text: r#"{"op":"deleteMany","filter":{"note":"do not delete this"}}"#.into(),
            mode: mystique_connector_api::QueryMode::Write,
            max_rows: None,
            timeout_ms: None,
            profile_read_only: false,
            confirm_object_name: None,
            collection: None,
            database: None,
        })
        .await
        .expect_err("deleteMany prompts");
    let msg = err.to_string();
    assert!(msg.contains("writeGuard"), "{msg}");
    assert!(msg.contains("unlock"), "{msg}");
    assert!(
        !ran.load(std::sync::atomic::Ordering::SeqCst),
        "deleteMany must not run before unlock"
    );

    ran.store(false, std::sync::atomic::Ordering::SeqCst);
    let shell = mgr
        .execute_query(mystique_connector_api::ExecuteQueryRequest {
            session_id: "prod-mongo".into(),
            text: r#"db.users.deleteMany({status: "keep"})"#.into(),
            mode: mystique_connector_api::QueryMode::Write,
            max_rows: None,
            timeout_ms: None,
            profile_read_only: false,
            confirm_object_name: None,
            collection: None,
            database: None,
        })
        .await
        .expect_err("shell deleteMany prompts");
    assert!(shell.to_string().contains("writeGuard"), "{shell}");
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn mongo_filter_delete_text_is_a_read() {
    let mgr = ConnectionManager::bootstrap();
    let ran = insert_probe(&mgr, "prod-mongo", false).await;
    let result = mgr
        .execute_query(mystique_connector_api::ExecuteQueryRequest {
            session_id: "prod-mongo".into(),
            text: r#"{"name":"please delete this"}"#.into(),
            mode: mystique_connector_api::QueryMode::Write,
            max_rows: None,
            timeout_ms: None,
            profile_read_only: false,
            confirm_object_name: None,
            collection: None,
            database: None,
        })
        .await
        .expect("filter value is not a write");
    assert!(ran.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(
        result.message.as_deref(),
        Some(r#"{"name":"please delete this"}"#)
    );

    ran.store(false, std::sync::atomic::Ordering::SeqCst);
    mgr.execute_query(mystique_connector_api::ExecuteQueryRequest {
        session_id: "prod-mongo".into(),
        text: r#"db.users.find({name: "please deleteMany this"})"#.into(),
        mode: mystique_connector_api::QueryMode::Write,
        max_rows: None,
        timeout_ms: None,
        profile_read_only: false,
        confirm_object_name: None,
        collection: None,
        database: None,
    })
    .await
    .expect("shell find stays a read");
    assert!(ran.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn mongo_write_on_read_only_session_ignores_client_flag() {
    let mgr = ConnectionManager::bootstrap();
    let ran = insert_probe(&mgr, "ro-mongo", true).await;
    let err = mgr
        .execute_query(mystique_connector_api::ExecuteQueryRequest {
            session_id: "ro-mongo".into(),
            text: r#"{"op":"deleteMany","filter":{}}"#.into(),
            mode: mystique_connector_api::QueryMode::Write,
            max_rows: None,
            timeout_ms: None,
            profile_read_only: false,
            confirm_object_name: None,
            collection: None,
            database: None,
        })
        .await
        .expect_err("live read-only blocks the write");
    assert!(
        err.to_string().to_ascii_lowercase().contains("read-only"),
        "{err}"
    );
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
}

fn write_req(session_id: &str, text: &str) -> mystique_connector_api::ExecuteQueryRequest {
    mystique_connector_api::ExecuteQueryRequest {
        session_id: session_id.into(),
        text: text.into(),
        mode: mystique_connector_api::QueryMode::Write,
        max_rows: None,
        timeout_ms: None,
        profile_read_only: false,
        confirm_object_name: None,
        collection: None,
        database: None,
    }
}
