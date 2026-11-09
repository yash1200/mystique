//! Live checks for batch execution: notices, RETURNING counts, cancel.
//!
//! `cargo test -p mystique-plugin-postgres --test execution -- --ignored`
//!
//! Uses the local `postgres` database. Scratch tables start with `mq_ea_`.

use mystique_connector_api::{
    gate_sql_batch_in, ConnectionConfig, ConnectorSession, DatabaseConnector, PoolConfig,
    QueryBatchResult, QueryControl, QueryMode, ValidatedBatch,
};
use mystique_plugin_postgres::PostgresConnector;
use mystique_sql::SqlDialect;
use serde_json::{json, Value};

async fn connect(max_size: u32) -> Box<dyn ConnectorSession> {
    let user = std::env::var("USER").unwrap_or_else(|_| "yashjohri".into());
    PostgresConnector
        .connect(&ConnectionConfig {
            engine: "postgres".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 5432,
                "database": "postgres",
                "username": user
            }),
            password: None,
            pool: PoolConfig {
                max_size,
                min_idle: 0,
                acquire_timeout_ms: 5_000,
                idle_timeout_secs: 10,
            },
        })
        .await
        .expect("connect")
}

fn batch(text: &str, mode: QueryMode, params: &[Value], max_rows: u32) -> ValidatedBatch {
    let plan = gate_sql_batch_in(text, SqlDialect::Postgres, mode, false, params)
        .unwrap_or_else(|err| panic!("gate refused {text:?}: {err}"));
    ValidatedBatch {
        statements: plan.statements,
        mode,
        max_rows,
        timeout_ms: 10_000,
        explain_executes: plan.explain_executes,
        collection: None,
        database: None,
        control: QueryControl::new(),
        progress: None,
    }
}

async fn run(session: &dyn ConnectorSession, text: &str, mode: QueryMode) -> QueryBatchResult {
    session
        .execute_batch(&batch(text, mode, &[], 50))
        .await
        .unwrap_or_else(|err| panic!("{text:?}: {err}"))
}

async fn backend_pid(session: &dyn ConnectorSession) -> String {
    let ran = run(session, "SELECT pg_backend_pid()::text", QueryMode::Read).await;
    ran.results[0].result.as_ref().expect("pid").rows[0][0]
        .as_str()
        .expect("pid text")
        .to_string()
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn notices_and_warnings_come_back_with_their_statement() {
    let session = connect(1).await;
    let session = &*session;
    let ran = run(
        session,
        "DO $$ BEGIN RAISE NOTICE 'hello %', 1; RAISE WARNING 'careful'; END $$;
         SELECT 1;
         DROP TABLE IF EXISTS mq_ea_no_such_table",
        QueryMode::Write,
    )
    .await;
    assert_eq!(ran.stopped_at, None, "{:?}", ran.results);
    assert_eq!(
        ran.results[0].notices,
        vec![
            "NOTICE: hello 1".to_string(),
            "WARNING: careful".to_string()
        ]
    );
    assert!(ran.results[1].notices.is_empty(), "{:?}", ran.results[1]);
    assert_eq!(ran.results[2].notices.len(), 1, "{:?}", ran.results[2]);
    assert!(
        ran.results[2].notices[0].contains("mq_ea_no_such_table"),
        "{:?}",
        ran.results[2].notices
    );

    // A failed statement keeps the notices it raised before the error.
    let failed = run(
        session,
        "DO $$ BEGIN RAISE NOTICE 'before'; RAISE EXCEPTION 'boom'; END $$",
        QueryMode::Write,
    )
    .await;
    assert_eq!(failed.stopped_at, Some(0));
    assert!(failed.results[0].error.is_some());
    assert_eq!(
        failed.results[0].notices,
        vec!["NOTICE: before".to_string()]
    );

    // The next run on the same connection starts clean.
    let clean = run(session, "SELECT 2", QueryMode::Read).await;
    assert!(clean.results[0].notices.is_empty());
    session.close().await.ok();
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn returning_reports_every_row_it_changed_not_the_preview_cap() {
    let session = connect(1).await;
    let session = &*session;
    run(
        session,
        "DROP TABLE IF EXISTS mq_ea_returning",
        QueryMode::Write,
    )
    .await;
    run(
        session,
        "CREATE TABLE mq_ea_returning (id int)",
        QueryMode::Write,
    )
    .await;

    let ran = session
        .execute_batch(&batch(
            "INSERT INTO mq_ea_returning SELECT generate_series(1, 120) RETURNING id",
            QueryMode::Write,
            &[],
            50,
        ))
        .await
        .expect("insert");
    let stmt = &ran.results[0];
    let result = stmt.result.as_ref().expect("rows");
    assert_eq!(result.rows.len(), 50);
    assert!(result.truncated);
    assert_eq!(stmt.affected_rows, Some(120));

    // Bound parameters take the extended-protocol path.
    let bound = session
        .execute_batch(&batch(
            "UPDATE mq_ea_returning SET id = id + :step RETURNING id",
            QueryMode::Write,
            &[json!(1000)],
            10,
        ))
        .await
        .expect("update");
    assert_eq!(bound.results[0].affected_rows, Some(120));
    assert_eq!(
        bound.results[0].result.as_ref().expect("rows").rows.len(),
        10
    );

    run(session, "DROP TABLE mq_ea_returning", QueryMode::Write).await;
    session.close().await.ok();
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn notices_survive_a_pool_of_several_connections() {
    let session = connect(3).await;
    let session = &*session;
    // Concurrent runs each see only their own notices.
    let runs = (0..6).map(|n| {
        let text = format!("DO $$ BEGIN RAISE NOTICE 'run {n}'; PERFORM pg_sleep(0.05); END $$");
        async move { (n, run(session, &text, QueryMode::Write).await) }
    });
    for (n, ran) in futures_util::future::join_all(runs).await {
        assert_eq!(ran.results[0].notices, vec![format!("NOTICE: run {n}")]);
    }
    session.close().await.ok();
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn a_cancelled_run_closes_its_connection_instead_of_pooling_it() {
    // One connection, so a pooled one would come straight back.
    let session = connect(1).await;
    let session = &*session;
    let first = backend_pid(session).await;
    assert_eq!(backend_pid(session).await, first, "a clean run is pooled");

    for mode in [QueryMode::Write, QueryMode::Read] {
        let before = backend_pid(session).await;
        let sleeping = batch(
            "SELECT count(*) FROM generate_series(1, 2000000000)",
            mode,
            &[],
            50,
        );
        let control = std::sync::Arc::clone(&sleeping.control);
        let canceller = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            control.cancel();
        });
        let started = std::time::Instant::now();
        let err = session
            .execute_batch(&sleeping)
            .await
            .expect_err("cancelled");
        canceller.await.expect("canceller");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "cancel did not stop the statement"
        );
        assert!(
            err.to_string().to_ascii_lowercase().contains("cancel"),
            "{err}"
        );

        let after = backend_pid(session).await;
        assert_ne!(
            after, before,
            "{mode:?}: the cancelled connection was pooled"
        );
        // The old backend is gone, not idle in the pool.
        let gone = run(
            session,
            &format!("SELECT count(*)::text FROM pg_stat_activity WHERE pid = {before}"),
            QueryMode::Read,
        )
        .await;
        let mut alive = gone.results[0].result.as_ref().expect("count").rows[0][0].clone();
        for _ in 0..20 {
            if alive == json!("0") {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let again = run(
                session,
                &format!("SELECT count(*)::text FROM pg_stat_activity WHERE pid = {before}"),
                QueryMode::Read,
            )
            .await;
            alive = again.results[0].result.as_ref().expect("count").rows[0][0].clone();
        }
        assert_eq!(
            alive,
            json!("0"),
            "{mode:?}: backend {before} still connected"
        );
    }
    session.close().await.ok();
}
