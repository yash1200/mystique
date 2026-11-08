//! A capped SELECT in the middle of a batch does not stop the batch.

mod common;

use std::collections::BTreeMap;

use common::{connect, count, run_batch, write};
use mystique_connector_api::{
    ApplyTableEditsInput, QueryMode, TableEditColumn, TableRowUpdate, ValidatedQuery,
};
use serde_json::json;

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn batch_continues_after_a_truncated_select() {
    let session = connect().await;
    let session = session.as_ref();
    write(session, "DROP TABLE IF EXISTS s4_trunc").await;
    write(session, "CREATE TABLE s4_trunc (id INT PRIMARY KEY)").await;
    write(
        session,
        "INSERT INTO s4_trunc VALUES (1), (2), (3), (4), (5)",
    )
    .await;

    let ran = run_batch(
        session,
        "SELECT id FROM s4_trunc ORDER BY id; INSERT INTO s4_trunc VALUES (6); SELECT COUNT(*) FROM s4_trunc",
        QueryMode::Write,
        2,
    )
    .await;
    assert_eq!(ran.stopped_at, None, "{:?}", ran.results);
    assert_eq!(ran.results.len(), 3, "{:?}", ran.results);
    let first = ran.results[0].result.as_ref().expect("first set");
    assert!(first.truncated);
    assert_eq!(first.row_count, 2);
    assert_eq!(ran.results[1].affected_rows, Some(1));
    let last = ran.results[2].result.as_ref().expect("count set");
    assert_eq!(last.rows[0][0].to_string().trim_matches('"'), "6");

    // Same in Read mode, where the lock is checked between statements.
    let read = run_batch(
        session,
        "SELECT id FROM s4_trunc; SELECT 7",
        QueryMode::Read,
        2,
    )
    .await;
    assert_eq!(read.stopped_at, None, "{:?}", read.results);
    assert_eq!(read.results.len(), 2, "{:?}", read.results);

    // Single-result path returns the last statement.
    let single = session
        .execute_query(&ValidatedQuery::new(
            "SELECT id FROM s4_trunc; SELECT 42",
            QueryMode::Read,
            2,
            10_000,
        ))
        .await
        .expect("single path");
    assert_eq!(single.rows[0][0].to_string().trim_matches('"'), "42");

    // A capped last statement still leaves the pool usable.
    let capped = run_batch(session, "SELECT id FROM s4_trunc", QueryMode::Write, 2).await;
    assert!(capped.results[0]
        .result
        .as_ref()
        .is_some_and(|r| r.truncated));
    assert_eq!(count(session, "SELECT COUNT(*) FROM s4_trunc").await, 6);

    write(session, "DROP TABLE s4_trunc").await;
    session.close().await.ok();
}

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn grid_save_of_an_unchanged_value_succeeds() {
    let session = connect().await;
    let session = session.as_ref();
    write(session, "DROP TABLE IF EXISTS s4_found_rows").await;
    write(
        session,
        "CREATE TABLE s4_found_rows (id INT PRIMARY KEY, name VARCHAR(20))",
    )
    .await;
    write(session, "INSERT INTO s4_found_rows VALUES (1, 'same')").await;

    let column = |name: &str, pk: bool| TableEditColumn {
        name: name.into(),
        data_type: if pk { "int".into() } else { "varchar".into() },
        nullable: !pk,
        pk,
    };
    let update = TableRowUpdate {
        original: BTreeMap::from([
            ("id".to_string(), json!(1)),
            ("name".to_string(), json!("same")),
        ]),
        values: BTreeMap::from([("name".to_string(), json!("same"))]),
    };
    let saved = session
        .apply_table_edits(&ApplyTableEditsInput {
            table: "s4_found_rows".into(),
            schema: None,
            database: Some("mystique".into()),
            columns: vec![column("id", true), column("name", false)],
            updates: vec![update],
            inserts: Vec::new(),
            deletes: Vec::new(),
            dry_run: false,
        })
        .await
        .expect("saving an unchanged value");
    assert_eq!(saved.updated, 1);

    // The query pool still reports changed rows, not matched rows.
    let ran = run_batch(
        session,
        "UPDATE s4_found_rows SET name = 'same' WHERE id = 1",
        QueryMode::Write,
        100,
    )
    .await;
    assert_eq!(ran.results[0].affected_rows, Some(0));

    write(session, "DROP TABLE s4_found_rows").await;
    session.close().await.ok();
}
