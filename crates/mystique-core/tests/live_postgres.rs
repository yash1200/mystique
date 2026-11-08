//! Live Postgres checks for tool paging (R-08), early cancel (R-20), and
//! "no timeout" (B-08).
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
            query_timeout_secs: None,
            save_password: None,
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

async fn open_pg(mgr: &ConnectionManager, session_id: &str) {
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
        session_id: Some(session_id.into()),
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
}

fn sleep_req(session_id: &str, mode: QueryMode, timeout_ms: u64) -> ExecuteQueryRequest {
    ExecuteQueryRequest {
        session_id: session_id.into(),
        text: "SELECT pg_sleep(2)".into(),
        mode,
        max_rows: Some(1),
        timeout_ms: Some(timeout_ms),
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
    }
}

/// B-08: a profile timeout of 0 means none. The canvas path (a batch and
/// a pinned tab) must let a 2s sleep finish; a 1s limit stops it.
#[tokio::test]
#[ignore = "needs local Postgres"]
async fn a_zero_timeout_lets_pg_sleep_run_past_the_limit() {
    let mgr = ConnectionManager::bootstrap();
    // The limited run gets its own pool. A late cancel from its deadline can
    // land on the next statement of a pooled connection (a known issue).
    open_pg(&mgr, "b4-limited").await;
    open_pg(&mgr, "b4-timeout").await;

    // pg_sleep is not a proven read, so the canvas runs it in Write mode.
    let mode = QueryMode::Write;
    let started = Instant::now();
    let limited = mgr
        .execute_batch(sleep_req("b4-limited", mode, 1_000), None)
        .await;
    let message = match &limited {
        Err(err) => err.to_string(),
        Ok(ran) => ran
            .results
            .iter()
            .find_map(|stmt| stmt.error.clone())
            .expect("a 1s limit let a 2s sleep finish"),
    };
    let message = message.to_ascii_lowercase();
    assert!(
        message.contains("timeout") || message.contains("exceeded"),
        "{message}"
    );
    assert!(started.elapsed() < Duration::from_millis(1_900));
    mgr.close_session("b4-limited").await.unwrap();

    let started = Instant::now();
    let ran = mgr
        .execute_batch(sleep_req("b4-timeout", mode, 0), None)
        .await
        .unwrap_or_else(|err| panic!("no timeout still stopped: {err}"));
    assert!(
        ran.results.iter().all(|stmt| stmt.error.is_none()),
        "{:?}",
        ran.results
    );
    assert!(started.elapsed() >= Duration::from_secs(2));

    // 0 reaches the server as "off", not as a default limit.
    for (timeout_ms, want) in [(0, "0"), (1_000, "1s")] {
        let mut req = sleep_req("b4-timeout", QueryMode::Read, timeout_ms);
        req.text = "SHOW statement_timeout".into();
        let ran = mgr.execute_batch(req, None).await.expect("show");
        let cell = ran.results[0].result.as_ref().expect("rows").rows[0][0].clone();
        assert_eq!(cell, json!(want), "timeout_ms {timeout_ms}");
    }

    mgr.pin_tab("b4-timeout", "b4-tab").await.unwrap();
    let ran = mgr
        .execute_pinned_batch(
            sleep_req("b4-timeout", QueryMode::Write, 0),
            "b4-tab".into(),
            None,
        )
        .await
        .expect("pinned run with no timeout");
    assert!(ran.results.iter().all(|stmt| stmt.error.is_none()));
    let _ = mgr.rollback_tab("b4-tab").await;
    mgr.close_session("b4-timeout").await.unwrap();
}
