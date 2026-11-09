use mystique_connector_api::{
    ApplyDdlResult, ColumnDraft, ConnectorResult, ConnectorSession, DdlPlan, EngineCapabilities,
    IndexDraft, PoolConfig, PoolStatus, TableDefinition,
};
use serde_json::json;

use mystique_connector_api::ConnectorError;

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
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: None,
            params: Vec::new(),
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
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
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
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: None,
            params: Vec::new(),
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
        confirm_object_name: Some("t".into()),
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
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
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
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
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: None,
            params: Vec::new(),
        })
        .await
        .expect_err("client write flag must not override the session");
    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("read-only") || msg.contains("readonly") || msg.contains("nothing ran"),
        "{err}"
    );

    let count = mgr
        .execute_query(mystique_connector_api::ExecuteQueryRequest {
            session_id: "ro".into(),
            text: "SELECT COUNT(*) FROM t".into(),
            mode: mystique_connector_api::QueryMode::Write,
            max_rows: None,
            timeout_ms: None,
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: None,
            params: Vec::new(),
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

fn import_req(session_id: &str, path: &str) -> mystique_connector_api::ImportObjectRequest {
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
        execution_id: None,
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
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: None,
            params: Vec::new(),
        })
        .await
        .expect("count");
    count.rows[0][0].as_i64().expect("count cell")
}

#[tokio::test]
async fn import_into_production_needs_no_extra_prompt() {
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

    // The import dialog's "Import into production" step is the confirmation.
    mgr.open_session(sqlite_open(&db_path, "prod", "production", false))
        .await
        .expect("open prod");
    mgr.import_object(import_req("prod", &csv_path.to_string_lossy()), None)
        .await
        .expect("import");
    assert_eq!(count_rows(&mgr, "prod").await, 1);

    let _ = mgr.close_session("prod").await;
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&csv_path);
}

fn guard_prompt(err: ConnectorError) -> mystique_connector_api::WriteGuardPrompt {
    match err {
        ConnectorError::WriteGuard(prompt) => *prompt,
        other => panic!("expected a write guard prompt, got {other:?}"),
    }
}

fn confirmed(
    mut req: mystique_connector_api::ExecuteQueryRequest,
    name: &str,
) -> mystique_connector_api::ExecuteQueryRequest {
    req.confirm_object_name = Some(name.into());
    req
}

async fn seeded(path: &std::path::Path, sql: &str) -> ConnectionManager {
    let _ = std::fs::remove_file(path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(path, "seed", "local", false))
        .await
        .expect("open seed");
    mgr.execute_query(write_req("seed", sql))
        .await
        .expect("seed");
    mgr.close_session("seed").await.expect("close seed");
    mgr
}

#[tokio::test]
async fn staging_writes_like_local_and_asks_once_for_a_drop() {
    let path = temp_db("staging");
    let mgr = seeded(&path, "CREATE TABLE t (id INTEGER PRIMARY KEY)").await;
    mgr.open_session(sqlite_open(&path, "stage", "staging", false))
        .await
        .expect("open");
    mgr.execute_query(write_req("stage", "INSERT INTO t VALUES (1)"))
        .await
        .expect("staging write runs with no unlock");

    let prompt = guard_prompt(
        mgr.execute_query(write_req("stage", "DROP TABLE t"))
            .await
            .expect_err("drop asks"),
    );
    assert_eq!(prompt.kind, "confirm");
    assert_eq!(prompt.object_name.as_deref(), Some("t"));
    assert_eq!(prompt.statements, vec!["DROP TABLE t".to_string()]);
    assert_eq!(count_rows(&mgr, "stage").await, 1);

    mgr.execute_query(confirmed(write_req("stage", "DROP TABLE t"), "t"))
        .await
        .expect("drop after the dialog");
    let _ = mgr.close_session("stage").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn production_prompts_carry_sql_and_a_typed_drop_unlocks() {
    let path = temp_db("prod-policy");
    let mgr = seeded(
        &path,
        "CREATE TABLE t (id INTEGER PRIMARY KEY); CREATE TABLE u (id INTEGER)",
    )
    .await;
    mgr.open_session(sqlite_open(&path, "prod", "production", false))
        .await
        .expect("open prod");

    let prompt = guard_prompt(
        mgr.execute_query(write_req(
            "prod",
            "INSERT INTO t VALUES (1); INSERT INTO t VALUES (2)",
        ))
        .await
        .expect_err("first write asks"),
    );
    assert_eq!(prompt.kind, "unlock");
    assert_eq!(
        prompt.statements,
        vec![
            "INSERT INTO t VALUES (1)".to_string(),
            "INSERT INTO t VALUES (2)".to_string()
        ]
    );

    // A destructive statement asks for the name straight away: one dialog.
    let prompt = guard_prompt(
        mgr.execute_query(write_req("prod", "DROP TABLE u"))
            .await
            .expect_err("drop asks for the name"),
    );
    assert_eq!(prompt.kind, "typeConfirm");
    assert_eq!(prompt.object_name.as_deref(), Some("u"));
    mgr.execute_query(confirmed(write_req("prod", "DROP TABLE u"), "u"))
        .await
        .expect("typed drop runs");

    // The typed confirmation also allowed later writes.
    mgr.execute_query(write_req("prod", "INSERT INTO t VALUES (1)"))
        .await
        .expect("write after typed confirm");
    assert_eq!(count_rows(&mgr, "prod").await, 1);
    let _ = mgr.close_session("prod").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn production_manual_commit_tab_skips_the_unlock() {
    let path = temp_db("prod-pin");
    let mgr = seeded(&path, "CREATE TABLE t (id INTEGER PRIMARY KEY)").await;
    mgr.open_session(sqlite_open(&path, "prod", "production", false))
        .await
        .expect("open prod");
    mgr.pin_tab("prod", "tab-1").await.expect("pin");
    mgr.execute_pinned_batch(
        batch_req(
            "prod",
            "BEGIN; INSERT INTO t VALUES (1)",
            mystique_connector_api::QueryMode::Write,
        ),
        "tab-1".into(),
        None,
    )
    .await
    .expect("Commit is the decision in a manual-commit tab");
    mgr.commit_tab("tab-1").await.expect("commit");
    assert_eq!(count_rows(&mgr, "prod").await, 1);

    // Outside the pinned tab the session is still locked.
    let prompt = guard_prompt(
        mgr.execute_query(write_req("prod", "INSERT INTO t VALUES (2)"))
            .await
            .expect_err("unpinned write asks"),
    );
    assert_eq!(prompt.kind, "unlock");
    let _ = mgr.close_session("prod").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn production_grid_save_prompt_shows_the_save_sql() {
    let path = temp_db("prod-grid");
    let mgr = seeded(&path, "CREATE TABLE t (id INTEGER PRIMARY KEY, n INTEGER)").await;
    mgr.open_session(sqlite_open(&path, "prod", "production", false))
        .await
        .expect("open prod");
    let column = |name: &str, pk: bool| mystique_connector_api::TableEditColumn {
        name: name.into(),
        data_type: "integer".into(),
        nullable: true,
        pk,
    };
    let save = mystique_connector_api::ApplyTableEditsRequest {
        session_id: "prod".into(),
        table: "t".into(),
        schema: None,
        database: None,
        columns: vec![column("id", true), column("n", false)],
        updates: Vec::new(),
        inserts: vec![mystique_connector_api::TableRowInsert {
            values: [("id".into(), json!(1)), ("n".into(), json!(5))]
                .into_iter()
                .collect(),
        }],
        deletes: Vec::new(),
        confirm_object_name: None,
        dry_run: false,
    };
    let prompt = guard_prompt(
        mgr.apply_table_edits(save.clone())
            .await
            .expect_err("first production save asks"),
    );
    assert_eq!(prompt.kind, "unlock");
    assert!(
        prompt.statements.iter().any(|sql| sql.contains("INSERT")),
        "{:?}",
        prompt.statements
    );
    assert_eq!(count_rows(&mgr, "prod").await, 0);

    mgr.unlock_writes("prod").await.expect("unlock");
    mgr.apply_table_edits(save)
        .await
        .expect("save after unlock");
    assert_eq!(count_rows(&mgr, "prod").await, 1);
    let _ = mgr.close_session("prod").await;
    let _ = std::fs::remove_file(&path);
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
        .import_object(import_req("ro", &csv_path.to_string_lossy()), None)
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

#[test]
fn cockroach_overlay_hides_terminate_and_postgres_keeps_it() {
    let probe = GuardProbe {
        ran: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };
    let cockroach =
        super::overlay_session_caps(&probe, mystique_connector_api::product("cockroachdb"));
    assert_eq!(cockroach.engine, "cockroachdb");
    assert!(!cockroach.supports_terminate_session);
    assert!(cockroach.supports_server_metrics);
    let postgres = super::overlay_session_caps(&probe, mystique_connector_api::product("postgres"));
    assert!(postgres.supports_terminate_session);
    assert!(postgres.supports_server_metrics);
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

    async fn apply_ddl(
        &self,
        plan: &DdlPlan,
        _draft: &TableDefinition,
    ) -> ConnectorResult<ApplyDdlResult> {
        self.ran.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(ApplyDdlResult {
            applied: plan.statements.clone(),
            message: "ran".into(),
        })
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
            origin: super::SessionOrigin::Ui,
            profile_id: None,
            last_used: std::sync::Mutex::new(std::time::Instant::now()),
        },
    );
    ran
}

#[tokio::test]
async fn aggregate_out_on_production_prompts_once() {
    let mgr = ConnectionManager::bootstrap();
    let ran = insert_probe(&mgr, "prod-agg", false).await;
    let err = mgr
        .execute_query(mystique_connector_api::ExecuteQueryRequest {
            session_id: "prod-agg".into(),
            text: r#"db.orders.aggregate([{ $match: {} }, { $out: "archive" }])"#.into(),
            mode: mystique_connector_api::QueryMode::Write,
            max_rows: None,
            timeout_ms: None,
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: None,
            params: Vec::new(),
        })
        .await
        .expect_err("aggregate $out prompts");
    let msg = err.to_string();
    assert!(msg.contains("writeGuard"), "{msg}");
    assert!(msg.contains("aggregate"), "{msg}");
    assert!(
        !ran.load(std::sync::atomic::Ordering::SeqCst),
        "$out must not run before unlock"
    );

    let read_only = insert_probe(&mgr, "ro-bulk", true).await;
    let blocked = mgr
        .execute_query(mystique_connector_api::ExecuteQueryRequest {
            session_id: "ro-bulk".into(),
            text: r#"db.orders.bulkWrite([{ insertOne: { document: { a: 1 } } }])"#.into(),
            mode: mystique_connector_api::QueryMode::Write,
            max_rows: None,
            timeout_ms: None,
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: None,
            params: Vec::new(),
        })
        .await
        .expect_err("read-only bulkWrite");
    assert!(
        blocked.to_string().to_ascii_lowercase().contains("read"),
        "{blocked}"
    );
    assert!(!read_only.load(std::sync::atomic::Ordering::SeqCst));
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
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: None,
            params: Vec::new(),
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
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: None,
            params: Vec::new(),
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
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: None,
            params: Vec::new(),
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
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
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
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: None,
            params: Vec::new(),
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
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
    }
}

fn batch_req(
    session_id: &str,
    text: &str,
    mode: mystique_connector_api::QueryMode,
) -> mystique_connector_api::ExecuteQueryRequest {
    mystique_connector_api::ExecuteQueryRequest {
        session_id: session_id.into(),
        text: text.into(),
        mode,
        max_rows: Some(50),
        timeout_ms: Some(5_000),
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
    }
}

async fn open_local(mgr: &ConnectionManager, path: &std::path::Path, session_id: &str) {
    mgr.open_session(sqlite_open(path, session_id, "local", false))
        .await
        .expect("open");
}

#[tokio::test]
async fn read_mode_batch_names_the_write_and_runs_nothing() {
    let path = temp_db("batch-read");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    open_local(&mgr, &path, "batch-read").await;
    mgr.execute_query(write_req(
        "batch-read",
        "CREATE TABLE t (id INTEGER); INSERT INTO t VALUES (1)",
    ))
    .await
    .expect("seed");

    let err = mgr
        .execute_batch(
            batch_req(
                "batch-read",
                "SELECT 1; DELETE FROM t",
                mystique_connector_api::QueryMode::Read,
            ),
            None,
        )
        .await
        .expect_err("read mode stops before execute");
    let msg = err.to_string();
    assert!(msg.contains("Statement 2"), "{msg}");
    assert!(msg.to_ascii_lowercase().contains("nothing ran"), "{msg}");
    assert_eq!(count_rows(&mgr, "batch-read").await, 1);

    let _ = mgr.close_session("batch-read").await;
    let _ = std::fs::remove_file(&path);
}

/// F-06: a function ReadProof cannot show to be a read is refused before
/// anything runs. Had the batch run, SQLite would answer `no such function`
/// as a stopped batch with the first result kept, not a policy error.
#[tokio::test]
async fn read_mode_refuses_an_unproven_function_before_executing() {
    let path = temp_db("batch-proof");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    open_local(&mgr, &path, "batch-proof").await;

    let text = "SELECT 1;\nSELECT my_func(1)";
    let err = mgr
        .execute_batch(
            batch_req("batch-proof", text, mystique_connector_api::QueryMode::Read),
            None,
        )
        .await
        .expect_err("read proof stops before execute");
    let ipc = mystique_connector_api::IpcError::from(err);
    assert_eq!(ipc.kind, mystique_connector_api::IpcErrorKind::Policy);
    assert_eq!(
        ipc.message,
        "Statement 2: `my_func` is not a known read-only function; switch to Write mode to run it. Nothing ran."
    );
    assert_eq!(ipc.statement_index, Some(1));
    let position = ipc.position.expect("position");
    assert_eq!(&text[position.offset as usize..][..7], "my_func");

    // Write mode sends it to the engine, which has no such function.
    let batch = mgr
        .execute_batch(
            batch_req(
                "batch-proof",
                text,
                mystique_connector_api::QueryMode::Write,
            ),
            None,
        )
        .await
        .expect("write mode runs");
    assert_eq!(batch.stopped_at, Some(1));
    assert!(batch.results[0].result.is_some());

    // A read that only calls listed functions still runs in Read mode.
    let batch = mgr
        .execute_batch(
            batch_req(
                "batch-proof",
                "SELECT upper('a'), json_extract('{\"a\":1}', '$.a'), date('now')",
                mystique_connector_api::QueryMode::Read,
            ),
            None,
        )
        .await
        .expect("allowlisted read runs");
    assert_eq!(batch.stopped_at, None);

    let _ = mgr.close_session("batch-proof").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn write_mode_batch_keeps_the_select_and_the_delete_count() {
    let path = temp_db("batch-write");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    open_local(&mgr, &path, "batch-write").await;
    mgr.execute_query(write_req(
        "batch-write",
        "CREATE TABLE t (id INTEGER); INSERT INTO t VALUES (1)",
    ))
    .await
    .expect("seed");

    let batch = mgr
        .execute_batch(
            batch_req(
                "batch-write",
                "SELECT 1; DELETE FROM t",
                mystique_connector_api::QueryMode::Write,
            ),
            None,
        )
        .await
        .expect("batch");
    assert!(batch.stopped_at.is_none());
    assert_eq!(batch.results.len(), 2);
    let select = batch.results[0].result.as_ref().expect("select");
    assert_eq!(select.rows[0][0], json!(1));
    assert_eq!(batch.results[1].affected_rows, Some(1));
    assert_eq!(count_rows(&mgr, "batch-write").await, 0);

    let _ = mgr.close_session("batch-write").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn comment_and_returning_and_two_selects_keep_their_rows() {
    let path = temp_db("batch-rows");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    open_local(&mgr, &path, "batch-rows").await;
    mgr.execute_query(write_req("batch-rows", "CREATE TABLE t (id INTEGER)"))
        .await
        .expect("create");

    let commented = mgr
        .execute_batch(
            batch_req(
                "batch-rows",
                "-- note\nSELECT 7",
                mystique_connector_api::QueryMode::Write,
            ),
            None,
        )
        .await
        .expect("comment");
    assert_eq!(commented.results.len(), 1);
    assert_eq!(
        commented.results[0].result.as_ref().expect("row").rows[0][0],
        json!(7)
    );

    let returning = mgr
        .execute_batch(
            batch_req(
                "batch-rows",
                "INSERT INTO t (id) VALUES (7) RETURNING id",
                mystique_connector_api::QueryMode::Write,
            ),
            None,
        )
        .await
        .expect("returning");
    let returned = returning.results[0]
        .result
        .as_ref()
        .expect("returning rows");
    assert_eq!(returned.rows[0][0], json!(7));
    assert_eq!(returning.results[0].affected_rows, Some(1));

    let both = mgr
        .execute_batch(
            batch_req(
                "batch-rows",
                "SELECT 1; SELECT 2",
                mystique_connector_api::QueryMode::Read,
            ),
            None,
        )
        .await
        .expect("two selects");
    assert_eq!(both.results.len(), 2);
    assert_eq!(
        both.results[0].result.as_ref().expect("1").rows[0][0],
        json!(1)
    );
    assert_eq!(
        both.results[1].result.as_ref().expect("2").rows[0][0],
        json!(2)
    );

    let _ = mgr.close_session("batch-rows").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn begin_is_refused_and_a_later_write_does_not_run() {
    let path = temp_db("batch-begin");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    open_local(&mgr, &path, "batch-begin").await;
    let err = mgr
        .execute_batch(
            batch_req(
                "batch-begin",
                "BEGIN; CREATE TABLE should_not_exist (id INTEGER)",
                mystique_connector_api::QueryMode::Write,
            ),
            None,
        )
        .await
        .expect_err("begin");
    let msg = err.to_string();
    assert!(msg.contains("Statement 1"), "{msg}");
    assert!(msg.to_ascii_lowercase().contains("nothing ran"), "{msg}");

    let missing = mgr
        .execute_query(batch_req(
            "batch-begin",
            "SELECT COUNT(*) FROM should_not_exist",
            mystique_connector_api::QueryMode::Read,
        ))
        .await
        .expect_err("table was not created");
    assert!(
        missing
            .to_string()
            .to_ascii_lowercase()
            .contains("no such table"),
        "{missing}"
    );

    let _ = mgr.close_session("batch-begin").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn syntax_error_is_refused_in_read_mode_and_sent_to_the_engine_in_write_mode() {
    let path = temp_db("batch-syntax");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    open_local(&mgr, &path, "batch-syntax").await;
    mgr.execute_query(write_req(
        "batch-syntax",
        "CREATE TABLE t (id INTEGER); INSERT INTO t VALUES (1)",
    ))
    .await
    .expect("seed");

    // Read mode cannot prove a read from text it cannot parse.
    let sql = "SELECT 1; SELECT * FORM t";
    let err = mgr
        .execute_batch(
            batch_req("batch-syntax", sql, mystique_connector_api::QueryMode::Read),
            None,
        )
        .await
        .expect_err("syntax");
    match err {
        mystique_connector_api::ConnectorError::QueryAt {
            position,
            statement_index,
            ..
        } => {
            assert_eq!(statement_index, Some(1));
            let at = usize::try_from(position.offset).unwrap_or(0);
            assert!(
                sql.get(at..).unwrap_or("").starts_with("FORM"),
                "offset {at} in {sql:?}"
            );
        }
        other => panic!("expected a positioned error, got {other}"),
    }
    assert_eq!(count_rows(&mgr, "batch-syntax").await, 1);

    // R-04: Write mode sends unparsed text to the engine as an opaque write.
    let batch = mgr
        .execute_batch(
            batch_req(
                "batch-syntax",
                sql,
                mystique_connector_api::QueryMode::Write,
            ),
            None,
        )
        .await
        .expect("engine reports the error");
    assert_eq!(batch.stopped_at, Some(1));
    let failed = batch.results.last().expect("statement 2");
    assert_eq!(failed.kind, mystique_connector_api::StatementKind::Write);
    let message = failed.error.as_deref().unwrap_or("");
    assert!(message.contains("FORM"), "{message}");
    assert_eq!(count_rows(&mgr, "batch-syntax").await, 1);

    let _ = mgr.close_session("batch-syntax").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn a_later_statement_error_keeps_the_earlier_result() {
    let path = temp_db("batch-partial");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    open_local(&mgr, &path, "batch-partial").await;
    let batch = mgr
        .execute_batch(
            batch_req(
                "batch-partial",
                "SELECT 1; SELECT * FROM missing_table",
                mystique_connector_api::QueryMode::Write,
            ),
            None,
        )
        .await
        .expect("partial ok");
    assert_eq!(batch.stopped_at, Some(1));
    assert_eq!(batch.results.len(), 2);
    assert!(batch.results[0].error.is_none());
    assert_eq!(
        batch.results[0].result.as_ref().expect("select").rows[0][0],
        json!(1)
    );
    assert!(batch.results[1].error.is_some());

    let _ = mgr.close_session("batch-partial").await;
    let _ = std::fs::remove_file(&path);
}

async fn cell(mgr: &ConnectionManager, session_id: &str, sql: &str) -> serde_json::Value {
    let result = mgr
        .execute_query(batch_req(
            session_id,
            sql,
            mystique_connector_api::QueryMode::Read,
        ))
        .await
        .expect("read");
    result.rows[0][0].clone()
}

#[tokio::test]
async fn manual_commit_hides_an_update_until_commit() {
    let path = temp_db("pin-vis");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&path, "pin-vis", "local", false))
        .await
        .expect("open");
    mgr.open_session(sqlite_open(&path, "pin-other", "local", false))
        .await
        .expect("other");
    mgr.execute_query(write_req(
        "pin-vis",
        "CREATE TABLE t (id INTEGER PRIMARY KEY, a INTEGER); INSERT INTO t (id, a) VALUES (1, 0)",
    ))
    .await
    .expect("seed");
    mgr.pin_tab("pin-vis", "tab-1").await.expect("pin");
    mgr.execute_pinned_batch(
        batch_req(
            "pin-vis",
            "BEGIN; UPDATE t SET a = 1 WHERE id = 1",
            mystique_connector_api::QueryMode::Write,
        ),
        "tab-1".into(),
        None,
    )
    .await
    .expect("update");
    assert_eq!(
        cell(&mgr, "pin-other", "SELECT a FROM t WHERE id = 1").await,
        json!(0)
    );
    let status = mgr.commit_tab("tab-1").await.expect("commit");
    assert!(status.pinned);
    assert!(!status.in_transaction);
    assert_eq!(
        cell(&mgr, "pin-other", "SELECT a FROM t WHERE id = 1").await,
        json!(1)
    );
    let _ = mgr.close_session("pin-vis").await;
    let _ = mgr.close_session("pin-other").await;
    let _ = std::fs::remove_file(&path);
}

fn explain_req(
    session_id: &str,
    text: &str,
    analyze: bool,
) -> mystique_connector_api::ExplainQueryRequest {
    mystique_connector_api::ExplainQueryRequest {
        session_id: session_id.into(),
        text: text.into(),
        analyze,
        mode: mystique_connector_api::QueryMode::Write,
        timeout_ms: Some(5_000),
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
    }
}

#[tokio::test]
async fn explain_select_returns_a_tree() {
    let path = temp_db("explain-select");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&path, "explain-select", "local", false))
        .await
        .expect("open");
    mgr.execute_query(write_req(
        "explain-select",
        "CREATE TABLE t (id INTEGER PRIMARY KEY); INSERT INTO t (id) VALUES (1)",
    ))
    .await
    .expect("seed");
    let plan = mgr
        .explain_query(explain_req("explain-select", "SELECT * FROM t", false))
        .await
        .expect("explain");
    let rendered = format!("{:?}", plan.root).to_ascii_lowercase();
    assert!(
        rendered.contains("scan") || rendered.contains('t'),
        "{rendered}"
    );
    assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    assert!(!plan.raw.is_empty());
    let _ = mgr.close_session("explain-select").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn explain_analyze_delete_on_production_does_not_delete() {
    let path = temp_db("explain-prod");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&path, "explain-seed", "local", false))
        .await
        .expect("seed open");
    mgr.execute_query(write_req(
        "explain-seed",
        "CREATE TABLE t (id INTEGER PRIMARY KEY, a INTEGER); INSERT INTO t (id, a) VALUES (1, 0)",
    ))
    .await
    .expect("seed");
    let _ = mgr.close_session("explain-seed").await;
    mgr.open_session(sqlite_open(&path, "explain-prod", "production", false))
        .await
        .expect("open");

    let blocked = mgr
        .explain_query(explain_req(
            "explain-prod",
            "DELETE FROM t WHERE id = 1",
            true,
        ))
        .await
        .expect_err("production analyze needs a confirm");
    assert!(
        matches!(
            blocked,
            mystique_connector_api::ConnectorError::WriteGuard(_)
        ),
        "{blocked}"
    );
    assert_eq!(
        cell(&mgr, "explain-prod", "SELECT a FROM t WHERE id = 1").await,
        json!(0)
    );

    mgr.unlock_writes("explain-prod").await.expect("unlock");
    let plan = mgr
        .explain_query(explain_req(
            "explain-prod",
            "DELETE FROM t WHERE id = 1",
            true,
        ))
        .await
        .expect("explain after confirm");
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.contains("did not change")),
        "{:?}",
        plan.warnings
    );
    assert_eq!(
        cell(&mgr, "explain-prod", "SELECT a FROM t WHERE id = 1").await,
        json!(0)
    );
    let _ = mgr.close_session("explain-prod").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn explain_analyze_delete_on_a_read_only_profile_runs_nothing() {
    let path = temp_db("explain-ro");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&path, "explain-ro-seed", "local", false))
        .await
        .expect("seed open");
    mgr.execute_query(write_req(
        "explain-ro-seed",
        "CREATE TABLE t (id INTEGER PRIMARY KEY); INSERT INTO t (id) VALUES (1)",
    ))
    .await
    .expect("seed");
    let _ = mgr.close_session("explain-ro-seed").await;
    mgr.open_session(sqlite_open(&path, "explain-ro", "local", true))
        .await
        .expect("open read-only");
    let err = mgr
        .explain_query(explain_req(
            "explain-ro",
            "DELETE FROM t WHERE id = 1",
            true,
        ))
        .await
        .expect_err("read-only explain");
    let msg = err.to_string().to_ascii_lowercase();
    assert!(msg.contains("read") || msg.contains("nothing ran"), "{err}");
    assert_eq!(
        cell(&mgr, "explain-ro", "SELECT COUNT(*) FROM t").await,
        json!(1)
    );
    let _ = mgr.close_session("explain-ro").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn plain_explain_of_a_write_is_allowed_in_read_mode_and_runs_nothing() {
    let path = temp_db("explain-plain-ro");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&path, "explain-plain-seed", "local", false))
        .await
        .expect("seed open");
    mgr.execute_query(write_req(
        "explain-plain-seed",
        "CREATE TABLE t (id INTEGER PRIMARY KEY); INSERT INTO t (id) VALUES (1)",
    ))
    .await
    .expect("seed");
    let _ = mgr.close_session("explain-plain-seed").await;
    mgr.open_session(sqlite_open(&path, "explain-plain", "production", true))
        .await
        .expect("open read-only production");

    let mut req = explain_req("explain-plain", "DELETE FROM t WHERE id = 1", false);
    req.mode = mystique_connector_api::QueryMode::Read;
    let plan = mgr
        .explain_query(req)
        .await
        .expect("plain explain of a delete");
    assert!(!plan.raw.is_empty());
    assert_eq!(
        cell(&mgr, "explain-plain", "SELECT COUNT(*) FROM t").await,
        json!(1)
    );

    // Analyze would run it, so Read mode still refuses that.
    let mut analyze = explain_req("explain-plain", "DELETE FROM t WHERE id = 1", true);
    analyze.mode = mystique_connector_api::QueryMode::Read;
    assert!(mgr.explain_query(analyze).await.is_err());
    // Transaction control is never explained.
    let mut begin = explain_req("explain-plain", "BEGIN", false);
    begin.mode = mystique_connector_api::QueryMode::Read;
    assert!(mgr.explain_query(begin).await.is_err());
    let _ = mgr.close_session("explain-plain").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn explain_binds_named_params() {
    let path = temp_db("explain-params");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&path, "explain-params", "local", false))
        .await
        .expect("open");
    mgr.execute_query(write_req(
        "explain-params",
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
    ))
    .await
    .expect("seed");
    let text = "SELECT * FROM t WHERE id = :id AND name = :name";
    let missing = mgr
        .explain_query(explain_req("explain-params", text, false))
        .await
        .expect_err("no values");
    assert!(
        missing.to_string().to_ascii_lowercase().contains("missing"),
        "{missing}"
    );
    let mut req = explain_req("explain-params", text, false);
    req.params = vec![json!(1), json!("a")];
    let plan = mgr.explain_query(req).await.expect("explain with params");
    let rendered = format!("{:?}", plan.root).to_ascii_lowercase();
    assert!(
        rendered.contains("primary key") || rendered.contains("search"),
        "{rendered}"
    );
    let _ = mgr.close_session("explain-params").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn closing_the_pin_rolls_the_update_back() {
    let path = temp_db("pin-rollback");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&path, "pin-rb", "local", false))
        .await
        .expect("open");
    mgr.open_session(sqlite_open(&path, "pin-rb-other", "local", false))
        .await
        .expect("other");
    mgr.execute_query(write_req(
        "pin-rb",
        "CREATE TABLE t (id INTEGER PRIMARY KEY, a INTEGER); INSERT INTO t (id, a) VALUES (1, 0)",
    ))
    .await
    .expect("seed");
    mgr.pin_tab("pin-rb", "tab-rb").await.expect("pin");
    mgr.execute_pinned_batch(
        batch_req(
            "pin-rb",
            "BEGIN; UPDATE t SET a = 1 WHERE id = 1",
            mystique_connector_api::QueryMode::Write,
        ),
        "tab-rb".into(),
        None,
    )
    .await
    .expect("update");
    let unpinned = mgr.rollback_tab("tab-rb").await.expect("unpin");
    assert!(
        unpinned.rolled_back,
        "unpin dropped an open transaction silently"
    );
    let status = mgr.pin_status("tab-rb").await;
    assert!(!status.pinned);
    assert!(!status.rolled_back, "the unpin already reported it");
    assert_eq!(
        cell(&mgr, "pin-rb-other", "SELECT a FROM t WHERE id = 1").await,
        json!(0)
    );
    let _ = mgr.close_session("pin-rb").await;
    let _ = mgr.close_session("pin-rb-other").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn idle_pin_rolls_back() {
    let path = temp_db("pin-idle");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&path, "pin-idle", "local", false))
        .await
        .expect("open");
    mgr.open_session(sqlite_open(&path, "pin-idle-other", "local", false))
        .await
        .expect("other");
    mgr.execute_query(write_req(
        "pin-idle",
        "CREATE TABLE t (id INTEGER PRIMARY KEY, a INTEGER); INSERT INTO t (id, a) VALUES (1, 0)",
    ))
    .await
    .expect("seed");
    mgr.pin_tab_for("pin-idle", "tab-idle", std::time::Duration::from_millis(1))
        .await
        .expect("pin");
    mgr.execute_pinned_batch(
        batch_req(
            "pin-idle",
            "BEGIN; UPDATE t SET a = 1 WHERE id = 1",
            mystique_connector_api::QueryMode::Write,
        ),
        "tab-idle".into(),
        None,
    )
    .await
    .expect("update");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    mgr.reap_idle().await;
    let status = mgr.pin_status("tab-idle").await;
    assert!(!status.pinned);
    assert!(status.rolled_back, "idle rollback was silent");
    assert_eq!(
        cell(&mgr, "pin-idle-other", "SELECT a FROM t WHERE id = 1").await,
        json!(0)
    );

    // Commit after the reap must not report success.
    let err = mgr.commit_tab("tab-idle").await.expect_err("commit");
    assert!(err.to_string().contains("rolled back"), "{err}");

    // The next run is refused once, so it does not run outside the lost transaction.
    mgr.reap_idle().await;
    mgr.pin_tab_for("pin-idle", "tab-idle", std::time::Duration::from_millis(1))
        .await
        .expect("pin");
    mgr.execute_pinned_batch(
        batch_req(
            "pin-idle",
            "BEGIN; UPDATE t SET a = 2 WHERE id = 1",
            mystique_connector_api::QueryMode::Write,
        ),
        "tab-idle".into(),
        None,
    )
    .await
    .expect("update");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    mgr.reap_idle().await;
    let run = || {
        mgr.execute_pinned_batch(
            batch_req(
                "pin-idle",
                "UPDATE t SET a = 3 WHERE id = 1",
                mystique_connector_api::QueryMode::Write,
            ),
            "tab-idle".into(),
            None,
        )
    };
    let err = run().await.expect_err("first run after the reap");
    assert!(err.to_string().contains("Nothing ran"), "{err}");
    assert_eq!(
        cell(&mgr, "pin-idle-other", "SELECT a FROM t WHERE id = 1").await,
        json!(0)
    );
    run().await.expect("second run");
    let _ = mgr.rollback_tab("tab-idle").await;
    let _ = mgr.close_session("pin-idle").await;
    let _ = mgr.close_session("pin-idle-other").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn a_temp_table_stays_on_the_pinned_tab() {
    let path = temp_db("pin-temp");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&path, "pin-temp", "local", false))
        .await
        .expect("open");
    mgr.open_session(sqlite_open(&path, "pin-temp-other", "local", false))
        .await
        .expect("other");
    mgr.pin_tab("pin-temp", "tab-temp").await.expect("pin");
    mgr.execute_pinned_batch(
        batch_req(
            "pin-temp",
            "CREATE TEMP TABLE mystique_pin (id INTEGER); INSERT INTO mystique_pin (id) VALUES (7)",
            mystique_connector_api::QueryMode::Write,
        ),
        "tab-temp".into(),
        None,
    )
    .await
    .expect("temp");
    let seen = mgr
        .execute_pinned_batch(
            batch_req(
                "pin-temp",
                "SELECT id FROM mystique_pin",
                mystique_connector_api::QueryMode::Read,
            ),
            "tab-temp".into(),
            None,
        )
        .await
        .expect("same tab");
    assert_eq!(
        seen.results[0].result.as_ref().expect("rows").rows[0][0],
        json!(7)
    );
    // The other tab is not pinned, so it must not see the temp table.
    // Use the other session id, which has its own connections.
    let err = mgr
        .execute_query(batch_req(
            "pin-temp-other",
            "SELECT id FROM mystique_pin",
            mystique_connector_api::QueryMode::Read,
        ))
        .await
        .expect_err("other connection");
    assert!(
        err.to_string()
            .to_ascii_lowercase()
            .contains("mystique_pin")
            || err
                .to_string()
                .to_ascii_lowercase()
                .contains("no such table"),
        "{err}"
    );
    let _ = mgr.rollback_tab("tab-temp").await;
    let _ = mgr.close_session("pin-temp").await;
    let _ = mgr.close_session("pin-temp-other").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn read_only_pin_rejects_a_write_before_execute() {
    let path = temp_db("pin-ro");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&path, "pin-ro", "local", true))
        .await
        .expect("open");
    mgr.execute_query(batch_req(
        "pin-ro",
        "CREATE TABLE t (id INTEGER PRIMARY KEY, a INTEGER)",
        mystique_connector_api::QueryMode::Read,
    ))
    .await
    .expect_err("create is a write");
    // Seed through a write session, then pin the read-only one.
    mgr.open_session(sqlite_open(&path, "pin-ro-write", "local", false))
        .await
        .expect("writer");
    mgr.execute_query(write_req(
        "pin-ro-write",
        "CREATE TABLE t (id INTEGER PRIMARY KEY, a INTEGER); INSERT INTO t (id, a) VALUES (1, 0)",
    ))
    .await
    .expect("seed");
    mgr.pin_tab("pin-ro", "tab-ro").await.expect("pin");
    let err = mgr
        .execute_pinned_batch(
            batch_req(
                "pin-ro",
                "UPDATE t SET a = 1 WHERE id = 1",
                mystique_connector_api::QueryMode::Write,
            ),
            "tab-ro".into(),
            None,
        )
        .await
        .expect_err("read-only");
    assert!(
        err.to_string().to_ascii_lowercase().contains("read"),
        "{err}"
    );
    assert_eq!(
        cell(&mgr, "pin-ro-write", "SELECT a FROM t WHERE id = 1").await,
        json!(0)
    );
    let _ = mgr.rollback_tab("tab-ro").await;
    let _ = mgr.close_session("pin-ro").await;
    let _ = mgr.close_session("pin-ro-write").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn autocommit_does_not_leave_a_begin_open() {
    let path = temp_db("pin-auto");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(sqlite_open(&path, "pin-auto", "local", false))
        .await
        .expect("open");
    mgr.execute_query(batch_req(
        "pin-auto",
        "SELECT 1",
        mystique_connector_api::QueryMode::Write,
    ))
    .await
    .expect("select");
    let err = mgr
        .execute_batch(
            batch_req(
                "pin-auto",
                "BEGIN",
                mystique_connector_api::QueryMode::Write,
            ),
            None,
        )
        .await
        .expect_err("begin refused");
    assert!(
        err.to_string().to_ascii_lowercase().contains("transaction"),
        "{err}"
    );
    mgr.open_session(sqlite_open(&path, "pin-auto-other", "local", false))
        .await
        .expect("other");
    mgr.execute_query(write_req("pin-auto-other", "CREATE TABLE t (id INTEGER)"))
        .await
        .expect("no leftover lock");
    let _ = mgr.close_session("pin-auto").await;
    let _ = mgr.close_session("pin-auto-other").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn two_get_or_open_calls_keep_one_session() {
    let path = temp_db("pin-once");
    let _ = std::fs::remove_file(&path);
    let mgr = std::sync::Arc::new(ConnectionManager::bootstrap());
    let req = sqlite_open(&path, "pin-once", "local", false);
    let first = mgr
        .get_or_open(req.clone(), super::SessionOrigin::Ui)
        .await
        .expect("first");
    let second = mgr
        .get_or_open(req, super::SessionOrigin::Ui)
        .await
        .expect("second");
    assert_eq!(first.opened_at, second.opened_at);
    mgr.execute_query(batch_req(
        "pin-once",
        "SELECT 1",
        mystique_connector_api::QueryMode::Read,
    ))
    .await
    .expect("still open");
    let _ = mgr.close_session("pin-once").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn closing_one_session_leaves_another_query_running() {
    let path_a = temp_db("pin-a");
    let path_b = temp_db("pin-b");
    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
    let mgr = std::sync::Arc::new(ConnectionManager::bootstrap());
    mgr.open_session(sqlite_open(&path_a, "sess-a", "local", false))
        .await
        .expect("a");
    mgr.open_session(sqlite_open(&path_b, "sess-b", "local", false))
        .await
        .expect("b");
    let worker = std::sync::Arc::clone(&mgr);
    let running = tokio::spawn(async move {
        worker
            .execute_query(batch_req(
                "sess-b",
                "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 200000) SELECT max(x) FROM c",
                mystique_connector_api::QueryMode::Read,
            ))
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    mgr.close_session("sess-a").await.expect("close a");
    let result = running.await.expect("join").expect("query b");
    assert_eq!(result.rows[0][0], json!(200000));
    let _ = mgr.close_session("sess-b").await;
    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
}

fn mongo_collection(name: &str, indexes: Vec<IndexDraft>) -> TableDefinition {
    let mut draft = TableDefinition::loaded(Some("app".into()), None, name, "collection");
    draft.indexes = indexes;
    draft
}

fn added_column(name: &str) -> ColumnDraft {
    ColumnDraft {
        name: name.into(),
        data_type: "text".into(),
        nullable: true,
        pk_position: 0,
        original: None,
        drop: false,
        ..ColumnDraft::default()
    }
}

#[tokio::test]
async fn a_structure_plan_with_warnings_never_runs() {
    let mgr = ConnectionManager::bootstrap();
    let ran = insert_probe(&mgr, "ddl-warn", false).await;
    let mut draft = mongo_collection("orders", Vec::new());
    draft.columns.push(added_column("note"));
    let err = mgr
        .apply_ddl(mystique_connector_api::ApplyDdlRequest {
            session_id: "ddl-warn".into(),
            draft,
            confirm_object_name: None,
        })
        .await
        .expect_err("warning blocks apply");
    assert!(
        err.to_string().to_ascii_lowercase().contains("column"),
        "{err}"
    );
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn structure_apply_on_production_prompts_and_read_only_runs_nothing() {
    let mgr = ConnectionManager::bootstrap();
    let index = IndexDraft {
        name: "note".into(),
        columns: vec!["note".into()],
        unique: false,
        primary: false,
        concurrent: false,
        original: None,
        drop: false,
        ..IndexDraft::default()
    };
    let ran = insert_probe(&mgr, "ddl-prod", false).await;
    let err = mgr
        .apply_ddl(mystique_connector_api::ApplyDdlRequest {
            session_id: "ddl-prod".into(),
            draft: mongo_collection("orders", vec![index.clone()]),
            confirm_object_name: None,
        })
        .await
        .expect_err("production prompts");
    assert!(err.to_string().contains("writeGuard"), "{err}");
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));

    let read_only = insert_probe(&mgr, "ddl-ro", true).await;
    let blocked = mgr
        .apply_ddl(mystique_connector_api::ApplyDdlRequest {
            session_id: "ddl-ro".into(),
            draft: mongo_collection("orders", vec![index]),
            confirm_object_name: None,
        })
        .await
        .expect_err("read-only");
    assert!(
        blocked.to_string().to_ascii_lowercase().contains("read"),
        "{blocked}"
    );
    assert!(!read_only.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn query_history_records_ui_agent_and_mcp_without_secrets() {
    let path = temp_db("history-v2");
    let _ = std::fs::remove_file(&path);
    let db = crate::storage::AppDatabase::open_in_memory().expect("app db");
    db.with_conn(|c| {
        c.execute(
            "INSERT INTO connections (name, engine, color_id, environment, read_only, plugin_config, created_at, updated_at)
             VALUES ('hist', 'sqlite', 'slate', 'local', 0, '{}', 'now', 'now')",
            [],
        )?;
        Ok(())
    })
    .expect("profile");
    let profile_id = "1".to_string();
    let services = crate::application::AppServices::with_secrets(
        db,
        ConnectionManager::bootstrap(),
        crate::secrets::SecretStore::memory(),
    );
    let mgr = services.connections();
    let open = |session_id: &str| {
        let mut req = sqlite_open(&path, session_id, "local", false);
        req.profile_id = Some(profile_id.clone());
        req.password = Some("pw-should-not-land".into());
        req
    };
    mgr.open_session(open("ui-hist")).await.expect("ui");
    mgr.get_or_open(open("agent:1"), super::SessionOrigin::Agent)
        .await
        .expect("agent");
    mgr.get_or_open(open("mcp:1"), super::SessionOrigin::Mcp)
        .await
        .expect("mcp");

    mgr.execute_query(write_req(
        "ui-hist",
        "CREATE TABLE t (id INTEGER); INSERT INTO t (id) VALUES (1)",
    ))
    .await
    .expect("dml");

    let mut agent = batch_req(
        "agent:1",
        "SELECT 'agent-marker'",
        mystique_connector_api::QueryMode::Read,
    );
    agent.database = Some("mongodb://alice:secret-token@127.0.0.1/app".into());
    mgr.execute_query(agent).await.expect("agent query");

    mgr.execute_query(batch_req(
        "mcp:1",
        "SELECT 'mcp-marker'",
        mystique_connector_api::QueryMode::Read,
    ))
    .await
    .expect("mcp query");

    mgr.execute_query(batch_req(
        "ui-hist",
        "SELECT 'mongodb://alice:secret-token@127.0.0.1/app'",
        mystique_connector_api::QueryMode::Read,
    ))
    .await
    .expect("uri sql");

    let page = services
        .history()
        .list_for_connection(&profile_id, 50)
        .expect("list");
    assert!(
        page.iter().any(|row| {
            row.source == crate::domain::QueryHistorySource::Ui
                && row.affected_rows == Some(1)
                && row.duration_ms.is_some()
                && row.body.contains("INSERT")
        }),
        "{page:?}"
    );
    assert!(page.iter().any(|row| {
        row.source == crate::domain::QueryHistorySource::Agent
            && row.body.contains("agent-marker")
            && row.database.is_none()
            && row.duration_ms.is_some()
    }));
    assert!(page.iter().any(|row| {
        row.source == crate::domain::QueryHistorySource::Mcp && row.body.contains("mcp-marker")
    }));

    let blob = services
        .db
        .with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT body || ' ' || ifnull(error_message,'') || ' ' || ifnull(\"database\",'') || ' ' || ifnull(engine,'') FROM query_history",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut out = String::new();
            for row in rows {
                out.push_str(&row?);
                out.push('\n');
            }
            Ok(out)
        })
        .expect("blob");
    assert!(!blob.contains("pw-should-not-land"), "{blob}");
    assert!(!blob.contains("secret-token"), "{blob}");
    assert!(blob.contains("alice:***@"), "{blob}");

    let _ = mgr.close_session("ui-hist").await;
    let _ = mgr.close_session("agent:1").await;
    let _ = mgr.close_session("mcp:1").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn history_logs_grid_saves_rollbacks_ddl_and_explain_analyze() {
    use crate::domain::QueryHistoryKind;

    let path = temp_db("history-kinds");
    let _ = std::fs::remove_file(&path);
    let db = crate::storage::AppDatabase::open_in_memory().expect("app db");
    db.with_conn(|c| {
        c.execute(
            "INSERT INTO connections (name, engine, color_id, environment, read_only, plugin_config, created_at, updated_at)
             VALUES ('kinds', 'sqlite', 'slate', 'local', 0, '{}', 'now', 'now')",
            [],
        )?;
        Ok(())
    })
    .expect("profile");
    let services = crate::application::AppServices::with_secrets(
        db,
        ConnectionManager::bootstrap(),
        crate::secrets::SecretStore::memory(),
    );
    let mgr = services.connections();
    let mut open = sqlite_open(&path, "kinds", "local", false);
    open.profile_id = Some("1".into());
    mgr.open_session(open).await.expect("open");
    mgr.execute_query(write_req(
        "kinds",
        "CREATE TABLE docs (id INTEGER PRIMARY KEY, n INTEGER); INSERT INTO docs VALUES (1, 10)",
    ))
    .await
    .expect("seed");

    let column = |name: &str| mystique_connector_api::TableEditColumn {
        name: name.into(),
        data_type: "integer".into(),
        nullable: true,
        pk: name == "id",
    };
    let saved = mgr
        .apply_table_edits(mystique_connector_api::ApplyTableEditsRequest {
            session_id: "kinds".into(),
            table: "docs".into(),
            schema: None,
            database: None,
            columns: vec![column("id"), column("n")],
            updates: vec![mystique_connector_api::TableRowUpdate {
                original: [("id".into(), json!(1)), ("n".into(), json!(10))]
                    .into_iter()
                    .collect(),
                values: [("n".into(), json!(11))].into_iter().collect(),
            }],
            inserts: Vec::new(),
            deletes: Vec::new(),
            confirm_object_name: None,
            dry_run: false,
        })
        .await
        .expect("grid save");
    mgr.rollback_table_edits(mystique_connector_api::RollbackTableEditsRequest {
        session_id: "kinds".into(),
        save_id: saved.save_id.expect("save id"),
        confirm_object_name: None,
    })
    .await
    .expect("rollback");

    let mut draft = mgr
        .describe_table_definition(mystique_connector_api::DescribeTableDefinitionRequest {
            session_id: "kinds".into(),
            table: "docs".into(),
            schema: None,
            database: None,
            kind: None,
        })
        .await
        .expect("describe");
    draft.columns.push(added_column("note"));
    mgr.apply_ddl(mystique_connector_api::ApplyDdlRequest {
        session_id: "kinds".into(),
        draft,
        confirm_object_name: None,
    })
    .await
    .expect("add column");

    mgr.explain_query(explain_req("kinds", "SELECT * FROM docs", true))
        .await
        .expect("explain analyze");
    // A plain explain runs nothing and is not logged.
    mgr.explain_query(explain_req("kinds", "SELECT 1", false))
        .await
        .expect("plain explain");

    let page = services
        .history()
        .list_for_connection("1", 50)
        .expect("list");
    let of = |kind: QueryHistoryKind| {
        page.iter()
            .filter(|row| row.kind == kind)
            .collect::<Vec<_>>()
    };
    let grid = of(QueryHistoryKind::GridSave);
    assert_eq!(grid.len(), 1, "{page:?}");
    assert!(grid[0].body.contains("UPDATE"), "{}", grid[0].body);
    assert_eq!(grid[0].affected_rows, Some(1));
    let rollback = of(QueryHistoryKind::Rollback);
    assert_eq!(rollback.len(), 1, "{page:?}");
    assert!(rollback[0].success);
    assert!(rollback[0].body.contains("UPDATE"), "{}", rollback[0].body);
    let ddl = of(QueryHistoryKind::Ddl);
    assert_eq!(ddl.len(), 1, "{page:?}");
    assert!(ddl[0].body.contains("note"), "{}", ddl[0].body);
    let analyze = of(QueryHistoryKind::ExplainAnalyze);
    assert_eq!(analyze.len(), 1, "{page:?}");
    assert_eq!(analyze[0].body, "EXPLAIN ANALYZE SELECT * FROM docs");
    assert!(page
        .iter()
        .all(|row| row.kind != QueryHistoryKind::Query || !row.body.contains("SELECT 1")));
    assert!(of(QueryHistoryKind::Query)
        .iter()
        .any(|row| row.body.contains("CREATE TABLE docs")));

    let _ = mgr.close_session("kinds").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn bound_parameter_is_not_spliced_into_history() {
    let path = temp_db("bind-hist");
    let _ = std::fs::remove_file(&path);
    let db = crate::storage::AppDatabase::open_in_memory().expect("app db");
    db.with_conn(|c| {
        c.execute(
            "INSERT INTO connections (name, engine, color_id, environment, read_only, plugin_config, created_at, updated_at)
             VALUES ('bind', 'sqlite', 'slate', 'local', 0, '{}', 'now', 'now')",
            [],
        )?;
        Ok(())
    })
    .expect("profile");
    let profile_id = "1".to_string();
    let services = crate::application::AppServices::with_secrets(
        db,
        ConnectionManager::bootstrap(),
        crate::secrets::SecretStore::memory(),
    );
    let mgr = services.connections();
    let mut open = sqlite_open(&path, "bind-hist", "local", false);
    open.profile_id = Some(profile_id.clone());
    mgr.open_session(open).await.expect("open");
    mgr.execute_query(write_req(
        "bind-hist",
        "CREATE TABLE t (id INTEGER); INSERT INTO t (id) VALUES (1)",
    ))
    .await
    .expect("seed");

    let mut bound = batch_req(
        "bind-hist",
        "SELECT :marker",
        mystique_connector_api::QueryMode::Read,
    );
    bound.params = vec![json!("secret-value-zz")];
    let batch = mgr.execute_batch(bound, None).await.expect("bound select");
    let cell = batch
        .results
        .first()
        .and_then(|stmt| stmt.result.as_ref())
        .and_then(|result| result.rows.first())
        .and_then(|row| row.first().cloned())
        .expect("cell");
    assert_eq!(cell, json!("secret-value-zz"));

    let mut smash = batch_req(
        "bind-hist",
        "SELECT :id",
        mystique_connector_api::QueryMode::Read,
    );
    smash.params = vec![json!("1; DROP TABLE t")];
    let smashed = mgr
        .execute_batch(smash, None)
        .await
        .expect("a value is not a second statement");
    assert_eq!(smashed.results.len(), 1);
    assert!(smashed.stopped_at.is_none());
    let smashed_cell = smashed
        .results
        .first()
        .and_then(|stmt| stmt.result.as_ref())
        .and_then(|result| result.rows.first())
        .and_then(|row| row.first().cloned())
        .expect("smashed cell");
    assert_eq!(smashed_cell, json!("1; DROP TABLE t"));
    assert_eq!(count_rows(&mgr, "bind-hist").await, 1);

    let missing = mgr
        .execute_batch(
            batch_req(
                "bind-hist",
                "SELECT :missing",
                mystique_connector_api::QueryMode::Read,
            ),
            None,
        )
        .await
        .expect_err("missing value blocks the run");
    assert!(missing.to_string().contains(":missing"), "{missing}");

    let explained = mgr
        .explain_query(explain_req("bind-hist", "SELECT :id", false))
        .await
        .expect_err("explain still requires the value");
    assert!(explained.to_string().contains(":id"), "{explained}");

    let page = services
        .history()
        .list_for_connection(&profile_id, 50)
        .expect("list");
    assert!(
        page.iter()
            .any(|row| row.body.contains(":marker") && !row.body.contains("secret-value-zz")),
        "{page:?}"
    );
    assert!(
        page.iter()
            .all(|row| !row.body.contains("secret-value-zz") && !row.body.contains("DROP TABLE")),
        "{page:?}"
    );
    assert!(
        page.iter().any(|row| row.body.contains("SELECT :id")),
        "{page:?}"
    );

    let _ = mgr.close_session("bind-hist").await;
    let _ = std::fs::remove_file(&path);
}

fn pinned_write(session_id: &str, text: &str) -> mystique_connector_api::ExecuteQueryRequest {
    batch_req(session_id, text, mystique_connector_api::QueryMode::Write)
}

#[tokio::test]
async fn manual_commit_opens_a_transaction_on_the_first_write() {
    let path = temp_db("pin-auto-begin");
    let _ = std::fs::remove_file(&path);
    let mgr = ConnectionManager::bootstrap();
    open_local(&mgr, &path, "pin-ab").await;
    open_local(&mgr, &path, "pin-ab-other").await;
    mgr.execute_query(write_req(
        "pin-ab",
        "CREATE TABLE t (id INTEGER PRIMARY KEY, a INTEGER); INSERT INTO t (id, a) VALUES (1, 0)",
    ))
    .await
    .expect("seed");
    mgr.pin_tab("pin-ab", "tab-ab").await.expect("pin");

    // A read does not open one.
    mgr.execute_pinned_batch(
        pinned_write("pin-ab", "SELECT a FROM t"),
        "tab-ab".into(),
        None,
    )
    .await
    .expect("read");
    assert!(!mgr.pin_status("tab-ab").await.in_transaction);

    // A write with no BEGIN does, and rollback discards it.
    mgr.execute_pinned_batch(
        pinned_write("pin-ab", "UPDATE t SET a = 1 WHERE id = 1"),
        "tab-ab".into(),
        None,
    )
    .await
    .expect("update");
    assert!(mgr.pin_status("tab-ab").await.in_transaction);
    assert_eq!(
        cell(&mgr, "pin-ab-other", "SELECT a FROM t").await,
        json!(0)
    );
    let status = mgr.rollback_open_tab("tab-ab").await.expect("rollback");
    assert!(status.pinned && !status.in_transaction);
    assert_eq!(
        cell(&mgr, "pin-ab-other", "SELECT a FROM t").await,
        json!(0)
    );

    // Commit keeps it.
    mgr.execute_pinned_batch(
        pinned_write("pin-ab", "UPDATE t SET a = 2 WHERE id = 1"),
        "tab-ab".into(),
        None,
    )
    .await
    .expect("update");
    mgr.commit_tab("tab-ab").await.expect("commit");
    assert_eq!(
        cell(&mgr, "pin-ab-other", "SELECT a FROM t").await,
        json!(2)
    );

    // A typed BEGIN … COMMIT still works and is not doubled.
    mgr.execute_pinned_batch(
        pinned_write("pin-ab", "BEGIN; UPDATE t SET a = 3 WHERE id = 1; COMMIT"),
        "tab-ab".into(),
        None,
    )
    .await
    .expect("typed transaction");
    assert!(!mgr.pin_status("tab-ab").await.in_transaction);
    assert_eq!(
        cell(&mgr, "pin-ab-other", "SELECT a FROM t").await,
        json!(3)
    );

    // Commit with nothing open does not claim success.
    let err = mgr.commit_tab("tab-ab").await.expect_err("nothing open");
    assert!(err.to_string().contains("Nothing was committed"), "{err}");

    let _ = mgr.rollback_tab("tab-ab").await;
    let _ = mgr.close_session("pin-ab").await;
    let _ = mgr.close_session("pin-ab-other").await;
    let _ = std::fs::remove_file(&path);
}

/// A pin whose transaction the "server" drops when a statement says `boom`.
struct DroppingPin {
    open: bool,
}

fn ok_result() -> mystique_connector_api::QueryResult {
    mystique_connector_api::QueryResult {
        engine: "sqlite".into(),
        mode: mystique_connector_api::QueryMode::Write,
        columns: Vec::new(),
        rows: Vec::new(),
        row_count: 0,
        truncated: false,
        duration_ms: 0,
        message: None,
    }
}

#[async_trait::async_trait]
impl mystique_connector_api::PinnedSession for DroppingPin {
    async fn execute(
        &mut self,
        batch: &mystique_connector_api::ValidatedBatch,
    ) -> ConnectorResult<mystique_connector_api::QueryBatchResult> {
        let mut results = Vec::new();
        for stmt in &batch.statements {
            if stmt.sql.contains("boom") {
                self.open = false;
                let err = mystique_connector_api::ConnectorError::Query("deadlock".into());
                results.push(mystique_connector_api::StatementResult::failed(
                    stmt, 0, &err,
                ));
                return Ok(mystique_connector_api::QueryBatchResult {
                    results,
                    stopped_at: Some(stmt.index),
                });
            }
            self.open = true;
            results.push(mystique_connector_api::StatementResult::success(
                stmt,
                0,
                ok_result(),
                None,
                Vec::new(),
            ));
        }
        Ok(mystique_connector_api::QueryBatchResult {
            results,
            stopped_at: None,
        })
    }

    async fn commit(&mut self) -> ConnectorResult<()> {
        self.open = false;
        Ok(())
    }

    async fn rollback(&mut self) -> ConnectorResult<()> {
        self.open = false;
        Ok(())
    }

    fn in_transaction(&self) -> bool {
        self.open
    }
}

struct DroppingSession;

#[async_trait::async_trait]
impl ConnectorSession for DroppingSession {
    fn engine_id(&self) -> &str {
        "sqlite"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::sqlite_defaults()
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

    async fn begin_pinned(
        &self,
    ) -> ConnectorResult<Box<dyn mystique_connector_api::PinnedSession + Send>> {
        Ok(Box::new(DroppingPin { open: false }))
    }
}

#[tokio::test]
async fn a_transaction_the_server_dropped_is_reported_and_commit_refuses() {
    let mgr = ConnectionManager::bootstrap();
    mgr.sessions.write().await.insert(
        "drop".into(),
        LiveSession {
            session: std::sync::Arc::new(DroppingSession),
            display_name: "drop".into(),
            opened_at: String::new(),
            product_id: "sqlite".into(),
            _tunnel: None,
            environment: "local".into(),
            read_only: false,
            connection_name: "drop".into(),
            origin: super::SessionOrigin::Ui,
            profile_id: None,
            last_used: std::sync::Mutex::new(std::time::Instant::now()),
        },
    );
    mgr.pin_tab("drop", "tab-drop").await.expect("pin");
    mgr.execute_pinned_batch(
        pinned_write("drop", "UPDATE t SET a = 1"),
        "tab-drop".into(),
        None,
    )
    .await
    .expect("write");
    assert!(mgr.pin_status("tab-drop").await.in_transaction);

    mgr.execute_pinned_batch(
        pinned_write("drop", "UPDATE t SET a = boom"),
        "tab-drop".into(),
        None,
    )
    .await
    .expect("batch with a failed statement");
    let status = mgr.pin_status("tab-drop").await;
    assert!(status.pinned && !status.in_transaction);
    assert!(status.rolled_back, "the dropped transaction was silent");

    let err = mgr.commit_tab("tab-drop").await.expect_err("commit");
    assert!(err.to_string().contains("rolled back"), "{err}");
    assert!(
        !mgr.pin_status("tab-drop").await.rolled_back,
        "reported once"
    );

    // A plain failure inside an open transaction is not a lost transaction.
    mgr.execute_pinned_batch(
        pinned_write("drop", "UPDATE t SET a = 2"),
        "tab-drop".into(),
        None,
    )
    .await
    .expect("write");
    mgr.rollback_open_tab("tab-drop").await.expect("rollback");
    assert!(!mgr.pin_status("tab-drop").await.rolled_back);
    let _ = mgr.rollback_tab("tab-drop").await;
}

/// #196: requests carry no read-only flag. Every write path reads the live
/// session's bit, which follows the stored profile.
#[tokio::test]
async fn read_only_session_blocks_grid_save_and_object_admin() {
    let path = temp_db("ro-paths");
    let mgr = seeded(&path, "CREATE TABLE t (id INTEGER PRIMARY KEY, n INTEGER)").await;
    mgr.open_session(sqlite_open(&path, "ro", "local", true))
        .await
        .expect("open read-only");

    let save = mystique_connector_api::ApplyTableEditsRequest {
        session_id: "ro".into(),
        table: "t".into(),
        schema: None,
        database: None,
        columns: vec![mystique_connector_api::TableEditColumn {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            pk: true,
        }],
        updates: Vec::new(),
        inserts: vec![mystique_connector_api::TableRowInsert {
            values: [("id".into(), json!(1))].into_iter().collect(),
        }],
        deletes: Vec::new(),
        confirm_object_name: None,
        dry_run: false,
    };
    let err = mgr.apply_table_edits(save).await.expect_err("grid save");
    assert!(
        err.to_string().to_ascii_lowercase().contains("read-only"),
        "{err}"
    );

    let drop = mystique_connector_api::TableAdminRequest {
        session_id: "ro".into(),
        op: mystique_connector_api::TableAdminOp::Drop,
        name: "t".into(),
        schema: None,
        database: None,
        kind: None,
        new_name: None,
        confirm_object_name: Some("t".into()),
    };
    let err = mgr.apply_table_admin(drop).await.expect_err("drop");
    assert!(
        err.to_string().to_ascii_lowercase().contains("read-only"),
        "{err}"
    );
    assert_eq!(count_rows(&mgr, "ro").await, 0);

    let _ = mgr.close_session("ro").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn a_profile_edit_reaches_the_open_session() {
    let path = temp_db("ro-edit");
    let mgr = seeded(&path, "CREATE TABLE t (id INTEGER PRIMARY KEY)").await;
    let mut open = sqlite_open(&path, "p1", "local", false);
    open.profile_id = Some("p1".into());
    mgr.open_session(open).await.expect("open");
    mgr.execute_query(write_req("p1", "INSERT INTO t VALUES (1)"))
        .await
        .expect("writes before the edit");

    assert_eq!(mgr.apply_profile_read_only("p1", true).await, 1);
    let err = mgr
        .execute_query(write_req("p1", "INSERT INTO t VALUES (2)"))
        .await
        .expect_err("read-only after the edit");
    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("read-only") || msg.contains("nothing ran"),
        "{err}"
    );
    assert_eq!(count_rows(&mgr, "p1").await, 1);

    assert_eq!(mgr.apply_profile_read_only("other", true).await, 0);
    assert_eq!(mgr.apply_profile_read_only("p1", false).await, 1);
    mgr.execute_query(write_req("p1", "INSERT INTO t VALUES (3)"))
        .await
        .expect("writes again");
    assert_eq!(count_rows(&mgr, "p1").await, 2);

    let _ = mgr.close_session("p1").await;
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_client_cannot_send_the_read_only_bit() {
    let req: OpenSessionRequest = serde_json::from_value(json!({
        "engine": "sqlite",
        "pluginConfig": {},
        "profileReadOnly": true,
    }))
    .expect("parse");
    // Skipped on the wire: only the stored profile fills it.
    assert!(!req.profile_read_only);
}
