//! Explain with bound parameters, and plain explain of a write.
//!
//! `cargo test -p mystique-plugin-mysql --test explain -- --ignored`
//!
//! Uses the local `mystique` database. Tables are prefixed `mq_ea_` and dropped.

mod common;

use common::{connect, count, write};
use mystique_connector_api::{gate_sql_batch_in, QueryControl, QueryMode, ValidatedQuery};
use mystique_sql::SqlDialect;
use serde_json::json;

fn explain_subject(text: &str, params: &[serde_json::Value]) -> ValidatedQuery {
    let plan = gate_sql_batch_in(text, SqlDialect::Mysql, QueryMode::Write, false, params)
        .unwrap_or_else(|err| panic!("{text}: {err}"));
    let stmt = &plan.statements[0];
    ValidatedQuery {
        text: stmt.sql.clone(),
        mode: QueryMode::Read,
        max_rows: 1,
        timeout_ms: 10_000,
        collection: None,
        database: None,
        control: QueryControl::new(),
        params: stmt.params.clone(),
    }
}

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn explain_binds_named_params_and_does_not_run_a_delete() {
    let session = connect().await;
    let session = session.as_ref();
    write(session, "DROP TABLE IF EXISTS mq_ea_explain").await;
    write(
        session,
        "CREATE TABLE mq_ea_explain (id INT PRIMARY KEY, name VARCHAR(20))",
    )
    .await;
    write(session, "INSERT INTO mq_ea_explain VALUES (7, 'seven')").await;

    let select = explain_subject(
        "SELECT * FROM mq_ea_explain WHERE id = :id AND name = :name",
        &[json!(7), json!("seven")],
    );
    let plan = session
        .explain_query(&select, false)
        .await
        .expect("explain with params");
    assert!(plan.raw.contains("mq_ea_explain"), "{}", plan.raw);
    let analyzed = session
        .explain_query(&select, true)
        .await
        .expect("analyze with params");
    assert!(analyzed.raw.contains("actual time"), "{}", analyzed.raw);

    let delete = explain_subject("DELETE FROM mq_ea_explain WHERE id = :id", &[json!(7)]);
    let plain = session
        .explain_query(&delete, false)
        .await
        .expect("plain explain of a delete");
    assert!(!plain.raw.is_empty());
    assert_eq!(
        count(session, "SELECT COUNT(*) FROM mq_ea_explain").await,
        1
    );

    write(session, "DROP TABLE mq_ea_explain").await;
    session.close().await.ok();
}
