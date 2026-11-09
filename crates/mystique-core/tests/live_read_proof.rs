//! Live F-06 checks: Read mode refuses a function ReadProof cannot show to be
//! a read before it reaches the server, and ordinary reads still run.
//!
//! `cargo test -p mystique-core --test live_read_proof -- --ignored`
//!
//! Uses the local `postgres` database and the MySQL `mystique` database.
//! Everything created is named `mq_fc_*` and dropped again.

use mystique_core::{
    ConnectionManager, ExecuteQueryRequest, IpcError, IpcErrorKind, OpenSessionRequest, PoolConfig,
    QueryBatchResult, QueryMode,
};
use serde_json::{json, Value as JsonValue};

fn pool() -> Option<PoolConfig> {
    Some(PoolConfig {
        max_size: 2,
        min_idle: 0,
        acquire_timeout_ms: 5_000,
        idle_timeout_secs: 0,
    })
}

async fn open(mgr: &ConnectionManager, engine: &str, config: JsonValue, password: Option<&str>) {
    mgr.open_session(OpenSessionRequest {
        engine: engine.into(),
        plugin_config: config,
        password: password.map(str::to_string),
        pool: pool(),
        session_id: Some(format!("mq_fc_{engine}")),
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

fn req(engine: &str, text: &str, mode: QueryMode) -> ExecuteQueryRequest {
    ExecuteQueryRequest {
        session_id: format!("mq_fc_{engine}"),
        text: text.into(),
        mode,
        max_rows: Some(50),
        timeout_ms: Some(10_000),
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
    }
}

async fn run(
    mgr: &ConnectionManager,
    engine: &str,
    text: &str,
    mode: QueryMode,
) -> QueryBatchResult {
    let batch = mgr
        .execute_batch(req(engine, text, mode), None)
        .await
        .unwrap_or_else(|err| panic!("{text}: {err}"));
    if let Some(stmt) = batch.results.iter().find(|stmt| stmt.error.is_some()) {
        panic!("{text}: {:?}", stmt.error);
    }
    batch
}

fn first_cell(batch: &QueryBatchResult) -> JsonValue {
    batch
        .results
        .last()
        .expect("result")
        .result
        .as_ref()
        .expect("rows")
        .rows[0][0]
        .clone()
}

async fn refused(mgr: &ConnectionManager, engine: &str, text: &str) -> IpcError {
    let err = mgr
        .execute_batch(req(engine, text, QueryMode::Read), None)
        .await
        .expect_err(text);
    let ipc = IpcError::from(err);
    assert_eq!(ipc.kind, IpcErrorKind::Policy, "{text}: {}", ipc.message);
    ipc
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn postgres_read_mode_refuses_nextval_and_still_reads() {
    let user = std::env::var("USER").unwrap_or_else(|_| "postgres".into());
    let mgr = ConnectionManager::bootstrap();
    open(
        &mgr,
        "postgres",
        json!({ "host": "127.0.0.1", "port": 5432, "database": "postgres", "username": user }),
        None,
    )
    .await;
    run(
        &mgr,
        "postgres",
        "DROP SEQUENCE IF EXISTS mq_fc_seq; CREATE SEQUENCE mq_fc_seq",
        QueryMode::Write,
    )
    .await;

    let ipc = refused(&mgr, "postgres", "SELECT nextval('mq_fc_seq')").await;
    assert_eq!(
        ipc.message,
        "`nextval` may write; switch to Write mode to run it. Nothing ran."
    );
    assert_eq!(ipc.position.expect("position").offset, 7);
    let ipc = refused(&mgr, "postgres", "SELECT 1; SELECT pg_advisory_lock(42)").await;
    assert_eq!(ipc.statement_index, Some(1));
    // The sequence never moved: nothing reached the server.
    let called = run(
        &mgr,
        "postgres",
        "SELECT is_called FROM mq_fc_seq",
        QueryMode::Read,
    )
    .await;
    assert_eq!(first_cell(&called), json!(false));

    // Ordinary reads, catalog queries included, still run in Read mode.
    for text in [
        "SELECT n.nspname, c.relname, pg_size_pretty(pg_total_relation_size(c.oid)) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'pg_catalog' ORDER BY pg_total_relation_size(c.oid) DESC LIMIT 5",
        "SELECT pid, state, now() - query_start FROM pg_stat_activity",
        "SELECT d::date FROM generate_series(now() - interval '3 days', now(), interval '1 day') AS d",
        "WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 5) SELECT sum(n) OVER (ORDER BY n) FROM r",
        "SELECT jsonb_build_object('a', 1) ->> 'a', '{\"b\":2}'::jsonb @> '{\"b\":2}'",
        "SELECT x.v FROM (VALUES (1), (2)) t(a) CROSS JOIN LATERAL (SELECT a * 2 AS v) x",
        "SELECT current_database(), current_user, version(), pg_backend_pid()",
        "SELECT table_name FROM information_schema.tables WHERE table_schema = 'information_schema' LIMIT 3",
        "SELECT last_value FROM mq_fc_seq",
        "EXPLAIN SELECT nextval('mq_fc_seq')",
        "SET search_path TO public; SELECT 1",
    ] {
        run(&mgr, "postgres", text, QueryMode::Read).await;
    }

    // Write mode is the way out.
    let next = run(
        &mgr,
        "postgres",
        "SELECT nextval('mq_fc_seq')",
        QueryMode::Write,
    )
    .await;
    assert_eq!(first_cell(&next), json!(1));
    run(
        &mgr,
        "postgres",
        "DROP SEQUENCE mq_fc_seq",
        QueryMode::Write,
    )
    .await;
    mgr.close_session("mq_fc_postgres").await.unwrap();
}

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn mysql_read_mode_refuses_get_lock_and_still_reads() {
    let mgr = ConnectionManager::bootstrap();
    open(
        &mgr,
        "mysql",
        json!({ "host": "127.0.0.1", "port": 3306, "database": "mystique", "username": "admin" }),
        Some("123"),
    )
    .await;

    let ipc = refused(&mgr, "mysql", "SELECT GET_LOCK('mq_fc_lock', 0)").await;
    assert_eq!(
        ipc.message,
        "`GET_LOCK` takes or releases a lock; switch to Write mode to run it. Nothing ran."
    );
    refused(&mgr, "mysql", "SELECT SLEEP(5)").await;
    refused(
        &mgr,
        "mysql",
        "EXPLAIN SELECT * FROM information_schema.tables WHERE table_name = (SELECT GET_LOCK('mq_fc_lock', 0))",
    )
    .await;
    // The lock was never taken.
    let free = run(
        &mgr,
        "mysql",
        "SELECT IS_FREE_LOCK('mq_fc_lock')",
        QueryMode::Write,
    )
    .await;
    assert_eq!(first_cell(&free).to_string().trim_matches('"'), "1");

    for text in [
        "SELECT table_name, table_rows FROM information_schema.tables WHERE table_schema = DATABASE() ORDER BY table_name LIMIT 5",
        "SELECT column_name, data_type FROM information_schema.columns WHERE table_schema = 'information_schema' LIMIT 5",
        "SELECT DATE_FORMAT(NOW(), '%Y-%m'), JSON_EXTRACT('{\"a\":1}', '$.a'), CONCAT_WS('-', 'a', 'b')",
        "WITH RECURSIVE seq (n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq WHERE n < 5) SELECT n, ROW_NUMBER() OVER (ORDER BY n DESC) FROM seq",
        "SELECT VERSION(), DATABASE(), CURRENT_USER(), CONNECTION_ID()",
        "SHOW TABLES",
        "SHOW VARIABLES LIKE 'max_connections'",
        "EXPLAIN SELECT 1",
    ] {
        run(&mgr, "mysql", text, QueryMode::Read).await;
    }
    mgr.close_session("mq_fc_mysql").await.unwrap();
}
