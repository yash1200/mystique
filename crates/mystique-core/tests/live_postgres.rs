//! Live Postgres checks for tool paging (R-08) and early cancel (R-20).
//!
//! `cargo test -p mystique-core --test live_postgres -- --ignored`
//!
//! Uses the local `postgres` database and drops its `s1_` scratch tables.

use std::time::{Duration, Instant};

use mystique_core::{
    AppDatabase, AppServices, ConnectionManager, ExecuteQueryRequest, OpenSessionRequest,
    PoolConfig, QueryMode, SaveConnectionInput, SecretStore, SshConfig, ToolCaller,
    SETTING_MCP_POLICY,
};
use serde_json::{json, Value as JsonValue};

fn user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "postgres".into())
}

fn pg_config() -> JsonValue {
    json!({
        "host": "127.0.0.1",
        "port": 5432,
        "database": "postgres",
        "username": user()
    })
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn paging_an_insert_returning_inserts_once() {
    let app = AppServices::with_secrets(
        AppDatabase::open_in_memory().unwrap(),
        ConnectionManager::bootstrap(),
        SecretStore::memory(),
    );
    app.settings()
        .set(SETTING_MCP_POLICY, "high_risk_write")
        .unwrap();
    let profile = app
        .profiles()
        .save(SaveConnectionInput {
            id: None,
            name: "s1-live".into(),
            engine: "postgres".into(),
            color_id: "teal".into(),
            environment: "local".into(),
            read_only: false,
            notes: None,
            plugin_config: pg_config(),
            password: None,
            secret_ref: None,
            clear_secret: false,
            copy_secret_from: None,
            favorite: None,
            ssh_config: SshConfig::default(),
            ssh_password: None,
            clear_ssh_secret: false,
            tls_key_passphrase: None,
        })
        .unwrap();
    let tools = app.agent_tools();
    let call = |query: &str, offset: u64| {
        let args = json!({
            "connection_id": profile.id,
            "query": query,
            "cell_char_offset": offset,
            "cell_char_limit": 10
        });
        let tools = tools.clone();
        async move {
            tools
                .call_tool("execute_query", &args, ToolCaller::Agent)
                .await
        }
    };
    call("DROP TABLE IF EXISTS s1_paging", 0).await.unwrap();
    call("CREATE TABLE s1_paging (v text)", 0).await.unwrap();

    let insert = format!(
        "INSERT INTO s1_paging (v) VALUES ('{}') RETURNING v",
        "abcdefghij".repeat(3)
    );
    let first = call(&insert, 0).await.unwrap();
    assert_eq!(first["rows"][0][0]["next_offset"], json!(10));
    let page = call(&insert, 10).await.unwrap();
    assert_eq!(page["rows"][0][0]["text"], json!("abcdefghij"));
    let refused = call("INSERT INTO s1_paging (v) VALUES ('x') RETURNING v", 10).await;
    assert!(refused.is_err());

    let count = call("SELECT count(*)::int AS n FROM s1_paging", 0)
        .await
        .unwrap();
    assert_eq!(count["rows"][0][0], json!(1), "paging ran the INSERT again");
    call("DROP TABLE s1_paging", 0).await.unwrap();
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn a_cancel_sent_before_the_run_starts_is_honoured() {
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(OpenSessionRequest {
        engine: "postgres".into(),
        plugin_config: pg_config(),
        password: None,
        pool: Some(PoolConfig {
            max_size: 2,
            min_idle: 0,
            acquire_timeout_ms: 5_000,
            idle_timeout_secs: 0,
        }),
        session_id: Some("s1-cancel".into()),
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

    mgr.cancel_query("s1-early").await.unwrap();
    let started = Instant::now();
    let err = mgr
        .execute_query(ExecuteQueryRequest {
            session_id: "s1-cancel".into(),
            text: "SELECT pg_sleep(5)".into(),
            mode: QueryMode::Read,
            max_rows: Some(1),
            timeout_ms: Some(30_000),
            profile_read_only: false,
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: Some("s1-early".into()),
            params: Vec::new(),
        })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("cancelled"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(1), "the sleep ran");
    mgr.close_session("s1-cancel").await.unwrap();
}
