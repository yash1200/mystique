//! Pinned (manual-commit) tabs: the first write opens a transaction (B-09).
//!
//! `cargo test -p mystique-plugin-mysql --test pinned -- --ignored`
//!
//! Uses the local `mystique` database. Tables are prefixed `b4_` and dropped.

mod common;

use common::{connect, count, write};
use mystique_connector_api::{
    gate_sql_batch_in, PinnedSession, QueryBatchResult, QueryControl, QueryMode, ValidatedBatch,
};
use mystique_sql::SqlDialect;

fn batch(sql: &str, timeout_ms: u64) -> ValidatedBatch {
    let plan =
        gate_sql_batch_in(sql, SqlDialect::Mysql, QueryMode::Write, true, &[]).expect("plan");
    ValidatedBatch {
        statements: plan.statements,
        mode: QueryMode::Write,
        max_rows: 100,
        timeout_ms,
        explain_executes: plan.explain_executes,
        collection: None,
        database: None,
        control: QueryControl::new(),
        progress: None,
    }
}

async fn on_pin(pin: &mut Box<dyn PinnedSession + Send>, sql: &str) -> QueryBatchResult {
    let ran = pin
        .execute(&batch(sql, 10_000))
        .await
        .unwrap_or_else(|err| panic!("{sql}: {err}"));
    for stmt in &ran.results {
        assert!(stmt.error.is_none(), "{sql}: {:?}", stmt.error);
    }
    ran
}

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn a_write_with_no_begin_waits_for_commit_or_rollback() {
    let session = connect().await;
    let observer = connect().await;
    write(&*observer, "DROP TABLE IF EXISTS b4_pin_auto").await;
    write(
        &*observer,
        "CREATE TABLE b4_pin_auto (id INT PRIMARY KEY) ENGINE = InnoDB",
    )
    .await;
    let rows = "SELECT COUNT(*) FROM b4_pin_auto";

    let mut pin = session.begin_pinned().await.expect("pin");
    on_pin(&mut pin, rows).await;
    assert!(!pin.in_transaction(), "a read opened a transaction");

    on_pin(&mut pin, "INSERT INTO b4_pin_auto VALUES (1)").await;
    assert!(pin.in_transaction(), "the write did not open a transaction");
    assert_eq!(count(&*observer, rows).await, 0, "visible before commit");
    pin.rollback().await.expect("rollback");
    assert!(!pin.in_transaction());
    assert_eq!(count(&*observer, rows).await, 0, "rollback kept the row");

    on_pin(&mut pin, "INSERT INTO b4_pin_auto VALUES (2)").await;
    pin.commit().await.expect("commit");
    assert_eq!(count(&*observer, rows).await, 1, "commit lost the row");

    // A typed START TRANSACTION still works and one COMMIT ends it.
    on_pin(
        &mut pin,
        "START TRANSACTION; INSERT INTO b4_pin_auto VALUES (3); COMMIT",
    )
    .await;
    assert!(!pin.in_transaction());
    assert_eq!(count(&*observer, rows).await, 2);

    // DDL commits implicitly. The write after it opens a new transaction.
    on_pin(
        &mut pin,
        "INSERT INTO b4_pin_auto VALUES (4); CREATE TABLE b4_pin_ddl (id INT); \
         INSERT INTO b4_pin_auto VALUES (5)",
    )
    .await;
    assert!(
        pin.in_transaction(),
        "the write after DDL ran outside a transaction"
    );
    pin.rollback().await.expect("rollback");
    let after_ddl = count(
        &*observer,
        "SELECT COUNT(*) FROM b4_pin_auto WHERE id IN (4, 5)",
    )
    .await;

    let err = pin.commit().await.expect_err("nothing open");
    assert!(err.to_string().contains("Nothing was committed"), "{err}");
    drop(pin);

    write(&*observer, "DROP TABLE IF EXISTS b4_pin_auto").await;
    write(&*observer, "DROP TABLE IF EXISTS b4_pin_ddl").await;
    assert_eq!(after_ddl, 1, "only the row before the DDL is committed");
    let _ = observer.close().await;
    let _ = session.close().await;
}

/// B-08: a run with no timeout sets no server limit on the pinned connection.
#[tokio::test]
#[ignore = "needs local MySQL"]
async fn no_timeout_sets_no_max_execution_time() {
    let session = connect().await;
    let mut pin = session.begin_pinned().await.expect("pin");
    for (timeout_ms, want) in [(0_u64, 0_i64), (1_500, 1_500)] {
        let ran = pin
            .execute(&batch("SELECT @@max_execution_time", timeout_ms))
            .await
            .expect("select");
        let cell = ran.results[0].result.as_ref().expect("rows").rows[0][0].clone();
        let value = cell
            .as_i64()
            .or_else(|| cell.as_str().and_then(|text| text.parse().ok()))
            .expect("number");
        assert_eq!(value, want, "timeout_ms {timeout_ms}");
    }
    drop(pin);
    let _ = session.close().await;
}
