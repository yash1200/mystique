//! Read mode is enforced by the server, not only by the classifier.
//! Statements are planned as Write so the classifier lets them through.

mod common;

use common::{connect, count, run, run_batch, write};
use mystique_connector_api::QueryMode;

const TABLE: &str = "s4_ro_probe";

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn read_mode_runs_selects_and_refuses_every_write() {
    let session = connect().await;
    let session = session.as_ref();
    write(session, &format!("DROP TABLE IF EXISTS {TABLE}")).await;
    write(session, "DROP TABLE IF EXISTS s4_ro_created").await;
    write(
        session,
        &format!("CREATE TABLE {TABLE} (id INT PRIMARY KEY, v INT)"),
    )
    .await;
    write(session, &format!("INSERT INTO {TABLE} VALUES (1, 1)")).await;

    let selected = run(session, &format!("SELECT v FROM {TABLE}"), QueryMode::Read)
        .await
        .expect("SELECT in Read mode");
    assert_eq!(selected.row_count, 1);
    let two = run(session, "SELECT 1; SELECT 2", QueryMode::Read)
        .await
        .expect("two SELECTs in Read mode");
    assert_eq!(two.rows[0][0].to_string().trim_matches('"'), "2");

    let refused = [
        format!("INSERT INTO {TABLE} VALUES (2, 2)"),
        format!("UPDATE {TABLE} SET v = 9"),
        format!("DELETE FROM {TABLE}"),
        "CREATE TABLE s4_ro_created (id INT)".to_string(),
        format!("DROP TABLE {TABLE}"),
        format!("TRUNCATE TABLE {TABLE}"),
        format!("COMMIT; INSERT INTO {TABLE} VALUES (3, 3)"),
        format!("START TRANSACTION READ WRITE; INSERT INTO {TABLE} VALUES (4, 4)"),
        format!("COMMIT; SET TRANSACTION READ WRITE; INSERT INTO {TABLE} VALUES (5, 5)"),
        format!("SET SESSION transaction_read_only = 0; INSERT INTO {TABLE} VALUES (6, 6)"),
    ];
    for sql in &refused {
        // Single-result path (MCP `execute_readonly_query`).
        let err = run(session, sql, QueryMode::Read).await;
        assert!(err.is_err(), "Read mode ran `{sql}`: {err:?}");
        // Batch path (query canvas).
        let batch = run_batch(session, sql, QueryMode::Read, 100).await;
        assert!(
            batch.stopped_at.is_some(),
            "batch ran `{sql}`: {:?}",
            batch.results
        );
    }

    assert_eq!(
        count(session, &format!("SELECT COUNT(*) FROM {TABLE}")).await,
        1
    );
    assert_eq!(
        count(session, &format!("SELECT SUM(v) FROM {TABLE}")).await,
        1
    );
    assert_eq!(
        count(
            session,
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = 's4_ro_created'"
        )
        .await,
        0
    );

    // The pool has one connection: Write mode after Read mode reuses it.
    write(session, &format!("INSERT INTO {TABLE} VALUES (7, 7)")).await;
    let batch = run_batch(
        session,
        &format!("UPDATE {TABLE} SET v = 8 WHERE id = 7"),
        QueryMode::Write,
        100,
    )
    .await;
    assert_eq!(batch.stopped_at, None, "{:?}", batch.results);
    let flag = run(session, "SELECT @@transaction_read_only", QueryMode::Write)
        .await
        .expect("read flag");
    assert_eq!(flag.rows[0][0].to_string().trim_matches('"'), "0");

    write(session, &format!("DROP TABLE {TABLE}")).await;
    session.close().await.ok();
}
