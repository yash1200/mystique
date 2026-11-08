//! Live checks for pinned-tab transactions and pooled session state.
//!
//! `cargo test -p mystique-plugin-postgres --test pinned_txn -- --ignored`
//!
//! Uses the local `postgres` database. Tables and roles are prefixed `s3_`
//! and dropped at the end.

use mystique_connector_api::{
    gate_sql_batch_in, ConnectionConfig, ConnectorSession, DatabaseConnector, PoolConfig,
    QueryBatchResult, QueryControl, QueryMode, SqlDialect, ValidatedBatch,
};
use mystique_plugin_postgres::PostgresConnector;
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

fn batch(text: &str, mode: QueryMode) -> ValidatedBatch {
    // Planned as a write so a Read-mode batch can carry what a classifier missed.
    let plan =
        gate_sql_batch_in(text, SqlDialect::Postgres, QueryMode::Write, true, &[]).expect("plan");
    ValidatedBatch {
        statements: plan.statements,
        mode,
        max_rows: 50,
        timeout_ms: 5_000,
        explain_executes: plan.explain_executes,
        collection: None,
        database: None,
        control: QueryControl::new(),
        progress: None,
    }
}

fn ok(result: QueryBatchResult) -> QueryBatchResult {
    for stmt in &result.results {
        assert!(stmt.error.is_none(), "statement failed: {:?}", stmt.error);
    }
    result
}

fn last_cell(result: &QueryBatchResult) -> Value {
    result
        .results
        .iter()
        .rev()
        .find_map(|stmt| stmt.result.as_ref())
        .and_then(|res| res.rows.first())
        .and_then(|row| row.first())
        .cloned()
        .expect("a row")
}

async fn scalar(session: &dyn ConnectorSession, sql: &str) -> Value {
    let result = session
        .execute_batch(&batch(sql, QueryMode::Write))
        .await
        .expect("query");
    last_cell(&ok(result))
}

async fn run(session: &dyn ConnectorSession, sql: &str) {
    ok(session
        .execute_batch(&batch(sql, QueryMode::Write))
        .await
        .expect("run"));
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn begin_then_select_keeps_the_transaction_and_commit_is_honest() {
    let session = connect(1).await;
    let mut pin = session.begin_pinned().await.expect("pin");

    ok(pin
        .execute(&batch("BEGIN; SELECT 1", QueryMode::Write))
        .await
        .expect("begin"));
    assert!(
        pin.in_transaction(),
        "BEGIN; SELECT left no open transaction"
    );
    pin.rollback().await.expect("rollback");
    assert!(!pin.in_transaction());

    let err = pin.commit().await.expect_err("commit with nothing open");
    assert!(err.to_string().contains("No open transaction"), "{err}");

    // The probe compares the transaction and statement clocks. Read it right
    // after BEGIN many times; it must never read as closed.
    for _ in 0..50 {
        ok(pin
            .execute(&batch("BEGIN", QueryMode::Write))
            .await
            .expect("begin"));
        assert!(pin.in_transaction(), "open transaction read as closed");
        pin.rollback().await.expect("rollback");
        assert!(!pin.in_transaction(), "closed transaction read as open");
    }

    ok(pin
        .execute(&batch("BEGIN", QueryMode::Write))
        .await
        .expect("begin"));
    ok(pin
        .execute(&batch("SELECT 1", QueryMode::Write))
        .await
        .expect("select"));
    assert!(pin.in_transaction());
    pin.commit().await.expect("commit");
    assert!(!pin.in_transaction());

    // A failed statement leaves an aborted transaction that still needs ROLLBACK.
    for commit in [false, true] {
        ok(pin
            .execute(&batch("BEGIN", QueryMode::Write))
            .await
            .expect("begin"));
        let failed = pin
            .execute(&batch("SELECT 1/0", QueryMode::Write))
            .await
            .expect("batch");
        assert!(failed.stopped_at.is_some());
        assert!(pin.in_transaction(), "aborted transaction reads as closed");
        if commit {
            // COMMIT of an aborted transaction rolls back. Commit must not claim success.
            let err = pin
                .commit()
                .await
                .expect_err("commit of a failed transaction");
            assert!(err.to_string().contains("Nothing was committed"), "{err}");
        } else {
            pin.rollback().await.expect("rollback aborted");
        }
        assert!(!pin.in_transaction());
    }

    // Unpin mid-transaction. The pool has one connection, so the next query
    // gets this one back. It must not still be inside the transaction.
    ok(pin
        .execute(&batch("BEGIN; SELECT 1", QueryMode::Write))
        .await
        .expect("begin"));
    let pid = scalar_on_pin(&mut pin, "SELECT pg_backend_pid()").await;
    drop(pin);
    let other = connect(1).await;
    run(&*session, "CREATE TABLE s3_pin_leak (id int)").await;
    assert_eq!(scalar(&*session, "SELECT pg_backend_pid()").await, pid);
    let seen = scalar(
        &*other,
        "SELECT to_regclass('public.s3_pin_leak') IS NOT NULL",
    )
    .await;
    run(&*session, "DROP TABLE IF EXISTS s3_pin_leak").await;
    assert_eq!(
        seen,
        json!(true),
        "pooled connection stayed in the transaction"
    );
    let _ = other.close().await;
    let _ = session.close().await;
}

async fn scalar_on_pin(
    pin: &mut Box<dyn mystique_connector_api::PinnedSession + Send>,
    sql: &str,
) -> Value {
    last_cell(&ok(pin
        .execute(&batch(sql, QueryMode::Write))
        .await
        .expect("pin query")))
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn select_after_update_does_not_commit_the_update() {
    let session = connect(2).await;
    let observer = connect(1).await;
    run(
        &*observer,
        "DROP TABLE IF EXISTS s3_pin_t; CREATE TABLE s3_pin_t (id int PRIMARY KEY, v int); \
         INSERT INTO s3_pin_t VALUES (1, 0)",
    )
    .await;

    let mut pin = session.begin_pinned().await.expect("pin");
    ok(pin
        .execute(&batch(
            "BEGIN; UPDATE s3_pin_t SET v = 1 WHERE id = 1",
            QueryMode::Write,
        ))
        .await
        .expect("update"));
    // Separate batch: the cursor must not be wrapped in BEGIN … COMMIT.
    assert_eq!(
        scalar_on_pin(&mut pin, "SELECT v FROM s3_pin_t").await,
        json!(1)
    );
    assert!(pin.in_transaction());
    let outside = scalar(&*observer, "SELECT v FROM s3_pin_t").await;
    pin.rollback().await.expect("rollback");
    let after = scalar(&*observer, "SELECT v FROM s3_pin_t").await;

    // Same batch: BEGIN, UPDATE, then a cursor SELECT.
    ok(pin
        .execute(&batch(
            "BEGIN; UPDATE s3_pin_t SET v = 2 WHERE id = 1; SELECT v FROM s3_pin_t",
            QueryMode::Write,
        ))
        .await
        .expect("same batch"));
    let same_batch = scalar(&*observer, "SELECT v FROM s3_pin_t").await;
    pin.rollback().await.expect("rollback");
    let pinned_after = scalar_on_pin(&mut pin, "SELECT v FROM s3_pin_t").await;
    drop(pin);

    run(&*observer, "DROP TABLE IF EXISTS s3_pin_t").await;
    assert_eq!(outside, json!(0), "SELECT committed the open UPDATE");
    assert_eq!(after, json!(0), "rollback did not undo the UPDATE");
    assert_eq!(
        same_batch,
        json!(0),
        "SELECT in the same batch committed the UPDATE"
    );
    assert_eq!(pinned_after, json!(0));
    let _ = observer.close().await;
    let _ = session.close().await;
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn unpin_resets_session_state_for_the_next_pool_user() {
    let session = connect(1).await;
    run(
        &*session,
        "DROP ROLE IF EXISTS s3_pin_role; CREATE ROLE s3_pin_role",
    )
    .await;

    let mut pin = session.begin_pinned().await.expect("pin");
    let pid = scalar_on_pin(&mut pin, "SELECT pg_backend_pid()").await;
    ok(pin
        .execute(&batch(
            "SET application_name = 's3_leak'; CREATE TEMP TABLE s3_tmp (id int); \
             SELECT pg_advisory_lock(31337); SET ROLE s3_pin_role",
            QueryMode::Write,
        ))
        .await
        .expect("session state"));
    assert!(!pin.in_transaction());
    pin.rollback().await.expect("unpin");
    drop(pin);

    let state = session
        .execute_batch(&batch(
            "SELECT pg_backend_pid(), current_setting('application_name'), current_user::text, \
             to_regclass('pg_temp.s3_tmp') IS NULL, \
             (SELECT count(*)::int FROM pg_locks WHERE locktype = 'advisory' AND pid = pg_backend_pid())",
            QueryMode::Write,
        ))
        .await
        .expect("state");
    let row = ok(state)
        .results
        .last()
        .and_then(|stmt| stmt.result.as_ref())
        .and_then(|res| res.rows.first().cloned())
        .expect("row");
    run(&*session, "DROP ROLE IF EXISTS s3_pin_role").await;
    let _ = session.close().await;

    assert_eq!(row[0], pid, "not the same pooled connection");
    assert_ne!(row[1], json!("s3_leak"), "SET leaked");
    assert_ne!(row[2], json!("s3_pin_role"), "SET ROLE leaked");
    assert_eq!(row[3], json!(true), "temp table leaked");
    assert_eq!(row[4], json!(0), "advisory lock leaked");
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn read_mode_advisory_lock_does_not_stay_on_the_pooled_connection() {
    let session = connect(1).await;
    let observer = connect(1).await;

    // The classifier may call this a read. The pooled connection must not keep it.
    let held = session
        .execute_batch(&batch("SELECT pg_advisory_lock(31338)", QueryMode::Read))
        .await
        .expect("read batch");
    ok(held);
    let query = mystique_connector_api::ValidatedQuery::new(
        "SELECT pg_advisory_lock(31339)",
        QueryMode::Read,
        10,
        5_000,
    );
    session.execute_query(&query).await.expect("read query");

    let free = scalar(
        &*observer,
        "SELECT pg_try_advisory_lock(31338) AND pg_try_advisory_lock(31339)",
    )
    .await;
    run(&*observer, "SELECT pg_advisory_unlock_all()").await;
    let _ = observer.close().await;
    let _ = session.close().await;
    assert_eq!(
        free,
        json!(true),
        "advisory lock stayed on the pooled connection"
    );
}
