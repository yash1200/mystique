//! Live checks for agent/MCP isolation (F-01) and access levels (F-02).
//!
//! `cargo test -p mystique-core --test live_tool_access -- --ignored`
//!
//! Postgres: local `postgres` database as `$USER`. MySQL: `admin`/`123`,
//! database `mystique`. Scratch tables start with `mq_fa_` and are dropped.

use std::sync::Arc;
use std::time::Duration;

use mystique_core::{
    AppDatabase, AppServices, ConnectionManager, ConnectionProfile, ExecuteQueryRequest,
    OpenSessionRequest, QueryMode, SaveConnectionInput, SecretStore, SshConfig, ToolCaller,
};
use serde_json::{json, Value as JsonValue};

fn app() -> Arc<AppServices> {
    Arc::new(AppServices::with_secrets(
        AppDatabase::open_in_memory().unwrap(),
        ConnectionManager::bootstrap(),
        SecretStore::memory(),
    ))
}

fn save(app: &AppServices, engine: &str, environment: &str) -> ConnectionProfile {
    let (config, password) = match engine {
        "mysql" => (
            json!({ "host": "127.0.0.1", "port": 3306, "database": "mystique", "username": "admin" }),
            Some("123".to_string()),
        ),
        _ => (
            json!({
                "host": "127.0.0.1",
                "port": 5432,
                "database": "postgres",
                "username": std::env::var("USER").unwrap_or_else(|_| "postgres".into())
            }),
            None,
        ),
    };
    app.profiles()
        .save(SaveConnectionInput {
            id: None,
            name: format!("mq_fa_{engine}"),
            engine: engine.into(),
            color_id: "teal".into(),
            environment: environment.into(),
            read_only: false,
            notes: None,
            plugin_config: config,
            password,
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
        .unwrap()
}

async fn mcp(app: &AppServices, id: &str, tool: &str, query: &str) -> Result<JsonValue, String> {
    app.agent_tools()
        .call_tool(
            tool,
            &json!({ "connection_id": id, "query": query }),
            ToolCaller::Mcp,
        )
        .await
        .map_err(|e| e.to_string())
}

fn level(app: &AppServices, id: &str, level: &str) {
    app.tool_access().set_level(id, None, Some(level)).unwrap();
}

/// Levels on a production connection: the explicit level is the grant, the
/// human's unlock is not, and each step up is needed.
async fn levels_on(engine: &str) {
    let app = app();
    let p = save(&app, engine, "production");
    level(&app, &p.id, "full");
    mcp(
        &app,
        &p.id,
        "execute_query",
        "DROP TABLE IF EXISTS mq_fa_levels",
    )
    .await
    .unwrap();
    mcp(
        &app,
        &p.id,
        "execute_query",
        "CREATE TABLE mq_fa_levels (id INT)",
    )
    .await
    .unwrap();

    // The human unlocks production on their own session.
    app.open_session(OpenSessionRequest {
        engine: engine.into(),
        plugin_config: p.plugin_config.clone(),
        password: None,
        pool: None,
        session_id: Some(p.id.clone()),
        profile_id: Some(p.id.clone()),
        ssh: None,
        tls_key_passphrase: None,
        environment: Some("production".into()),
        connection_name: Some(p.name.clone()),
        profile_read_only: false,
        use_saved_ssh: true,
    })
    .await
    .unwrap();
    app.connections().unlock_writes(&p.id).await.unwrap();

    level(&app, &p.id, "read");
    let err = mcp(
        &app,
        &p.id,
        "execute_query",
        "INSERT INTO mq_fa_levels VALUES (1)",
    )
    .await
    .unwrap_err();
    assert!(err.contains("needs `write`"), "{err}");

    level(&app, &p.id, "write");
    mcp(
        &app,
        &p.id,
        "execute_query",
        "INSERT INTO mq_fa_levels VALUES (1)",
    )
    .await
    .unwrap();
    let err = mcp(&app, &p.id, "execute_query", "DELETE FROM mq_fa_levels")
        .await
        .unwrap_err();
    assert!(err.contains("needs `full`"), "{err}");
    // Read mode on the server too: a write through the read tool fails.
    assert!(mcp(
        &app,
        &p.id,
        "execute_readonly_query",
        "INSERT INTO mq_fa_levels VALUES (2)"
    )
    .await
    .is_err());
    let rows = mcp(
        &app,
        &p.id,
        "execute_readonly_query",
        "SELECT count(*) FROM mq_fa_levels",
    )
    .await
    .unwrap();
    // MySQL sends the count as text.
    let n = &rows["rows"][0][0];
    assert!(n == &json!(1) || n == &json!("1"), "{rows}");

    level(&app, &p.id, "full");
    mcp(&app, &p.id, "execute_query", "DROP TABLE mq_fa_levels")
        .await
        .unwrap();
    let _ = app.connections().close_session(&p.id).await;
    let _ = app
        .connections()
        .close_session(&format!("mcp:{}", p.id))
        .await;
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn postgres_levels_and_no_inherited_unlock() {
    levels_on("postgres").await;
}

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn mysql_levels_and_no_inherited_unlock() {
    levels_on("mysql").await;
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn closing_the_human_session_does_not_cancel_an_mcp_query() {
    let app = app();
    let p = save(&app, "postgres", "local");
    level(&app, &p.id, "read");
    app.open_session(OpenSessionRequest {
        engine: "postgres".into(),
        plugin_config: p.plugin_config.clone(),
        password: None,
        pool: None,
        session_id: Some(p.id.clone()),
        profile_id: Some(p.id.clone()),
        ssh: None,
        tls_key_passphrase: None,
        environment: None,
        connection_name: None,
        profile_read_only: false,
        use_saved_ssh: true,
    })
    .await
    .unwrap();
    let running = {
        let app = Arc::clone(&app);
        let id = p.id.clone();
        tokio::spawn(async move {
            mcp(
                &app,
                &id,
                "execute_readonly_query",
                "SELECT count(*), 7 AS n FROM generate_series(1, 40000000)",
            )
            .await
        })
    };
    tokio::time::sleep(Duration::from_millis(400)).await;
    app.connections().close_session(&p.id).await.unwrap();
    let out = running.await.unwrap().expect("the MCP query finished");
    assert_eq!(out["rows"][0][1], json!(7), "{out}");

    // The human's own query is still cancelled by their disconnect.
    app.open_session(OpenSessionRequest {
        engine: "postgres".into(),
        plugin_config: p.plugin_config.clone(),
        password: None,
        pool: None,
        session_id: Some(p.id.clone()),
        profile_id: Some(p.id.clone()),
        ssh: None,
        tls_key_passphrase: None,
        environment: None,
        connection_name: None,
        profile_read_only: false,
        use_saved_ssh: true,
    })
    .await
    .unwrap();
    let human = {
        let app = Arc::clone(&app);
        let id = p.id.clone();
        tokio::spawn(async move {
            app.connections()
                .execute_query(ExecuteQueryRequest {
                    session_id: id,
                    text: "SELECT pg_sleep(3)".into(),
                    mode: QueryMode::Read,
                    max_rows: Some(1),
                    timeout_ms: Some(10_000),
                    confirm_object_name: None,
                    collection: None,
                    database: None,
                    execution_id: None,
                    params: Vec::new(),
                })
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(400)).await;
    app.connections().close_session(&p.id).await.unwrap();
    assert!(
        human.await.unwrap().is_err(),
        "the human query kept running"
    );
    let _ = app
        .connections()
        .close_session(&format!("mcp:{}", p.id))
        .await;
}

/// F-06 with levels: a UDF is not a provable read. It runs, in Write mode,
/// only where the caller may write; both the agent and MCP follow the rule.
#[tokio::test]
#[ignore = "needs local Postgres"]
async fn postgres_udf_runs_only_with_write_access() {
    let app = app();
    let p = save(&app, "postgres", "local");
    level(&app, &p.id, "full");
    mcp(
        &app,
        &p.id,
        "execute_query",
        "CREATE OR REPLACE FUNCTION mq_fa_udf() RETURNS int LANGUAGE sql AS 'SELECT 41 + 1'",
    )
    .await
    .unwrap();
    let call = |caller: ToolCaller, tool: &'static str| {
        let app = Arc::clone(&app);
        let id = p.id.clone();
        async move {
            app.agent_tools()
                .call_tool(
                    tool,
                    &json!({ "connection_id": id, "query": "SELECT mq_fa_udf() AS n" }),
                    caller,
                )
                .await
                .map_err(|e| e.to_string())
        }
    };
    level(&app, &p.id, "read");
    for caller in [ToolCaller::Mcp, ToolCaller::Agent] {
        let err = call(caller, "execute_query").await.unwrap_err();
        assert!(err.contains("needs `write`"), "{err}");
        assert!(err.contains("cannot prove it only reads"), "{err}");
        let err = call(caller, "execute_readonly_query").await.unwrap_err();
        assert!(err.contains("Use execute_query"), "{err}");
    }
    level(&app, &p.id, "write");
    for caller in [ToolCaller::Mcp, ToolCaller::Agent] {
        let out = call(caller, "execute_query").await.unwrap();
        assert_eq!(out["rows"][0][0], json!(42), "{out}");
    }
    level(&app, &p.id, "full");
    mcp(&app, &p.id, "execute_query", "DROP FUNCTION mq_fa_udf()")
        .await
        .unwrap();
    for sid in [format!("mcp:{}", p.id), format!("agent:{}", p.id)] {
        let _ = app.connections().close_session(&sid).await;
    }
}
