//! Live Postgres check for database-enforced read-only.
//!
//! `cargo test -p mystique-plugin-postgres --test read_only -- --ignored`
//!
//! Uses the local `postgres` database and drops its scratch table.

use mystique_connector_api::{
    ConnectionConfig, DatabaseConnector, PoolConfig, QueryControl, QueryMode, ValidatedQuery,
};
use mystique_plugin_postgres::PostgresConnector;
use serde_json::json;

fn query(text: &str, mode: QueryMode) -> ValidatedQuery {
    ValidatedQuery {
        text: text.into(),
        mode,
        max_rows: 50,
        timeout_ms: 5_000,
        collection: None,
        database: None,
        control: QueryControl::new(),
    }
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn read_only_mode_rejects_writes_and_releases_the_connection() {
    let user = std::env::var("USER").unwrap_or_else(|_| "yashjohri".into());
    let session = PostgresConnector
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
                max_size: 1,
                min_idle: 0,
                acquire_timeout_ms: 5_000,
                idle_timeout_secs: 10,
            },
        })
        .await
        .expect("connect");

    session
        .execute_query(&query(
            "DROP TABLE IF EXISTS ro_t; CREATE TABLE ro_t (id int); INSERT INTO ro_t VALUES (1)",
            QueryMode::Write,
        ))
        .await
        .expect("seed");

    let cte = session
        .execute_query(&query(
            "WITH d AS (DELETE FROM ro_t RETURNING id) SELECT * FROM d",
            QueryMode::Read,
        ))
        .await
        .expect_err("cte delete");
    let cte_msg = cte.to_string().to_ascii_lowercase();
    assert!(
        cte_msg.contains("read-only") || cte_msg.contains("read only"),
        "{cte}"
    );

    let explain = session
        .execute_query(&query(
            "EXPLAIN (ANALYZE) DELETE FROM ro_t",
            QueryMode::Read,
        ))
        .await
        .expect_err("explain analyze delete");
    let explain_msg = explain.to_string().to_ascii_lowercase();
    assert!(
        explain_msg.contains("read-only") || explain_msg.contains("read only"),
        "{explain}"
    );

    let count = session
        .execute_query(&query("SELECT COUNT(*)::int FROM ro_t", QueryMode::Read))
        .await
        .expect("count");
    assert_eq!(
        count.rows[0][0],
        json!(1),
        "read-only statements changed a row"
    );

    session
        .execute_query(&query("INSERT INTO ro_t VALUES (2)", QueryMode::Write))
        .await
        .expect("write after read-only transaction");

    let txn = session
        .execute_query(&query("SHOW transaction_read_only", QueryMode::Write))
        .await
        .expect("txn");
    assert_eq!(txn.rows[0][0], json!("off"));

    let _ = session
        .execute_query(&query("DROP TABLE ro_t", QueryMode::Write))
        .await;
    let _ = session.close().await;
}
