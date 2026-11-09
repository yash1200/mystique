//! Live checks for the grid's background count and value completion.
//!
//! `cargo test -p mystique-plugin-postgres --test grid_count -- --ignored`
//!
//! Uses the local `postgres` database. Scratch objects start with `mq_eg_`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mystique_connector_api::{
    ConnectionConfig, ConnectorSession, DatabaseConnector, DistinctColumnValuesInput,
    DistinctColumnValuesRequest, FetchTablePreviewInput, FetchTablePreviewStatsRequest, PoolConfig,
    QueryControl, QueryMode, TableFilter, ValidatedQuery,
};
use mystique_plugin_postgres::PostgresConnector;
use serde_json::json;

async fn connect() -> Box<dyn ConnectorSession> {
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
                max_size: 3,
                min_idle: 0,
                acquire_timeout_ms: 5_000,
                idle_timeout_secs: 10,
            },
        })
        .await
        .expect("connect")
}

async fn write(session: &dyn ConnectorSession, sql: &str) {
    session
        .execute_query(&ValidatedQuery::new(sql, QueryMode::Write, 10, 30_000))
        .await
        .unwrap_or_else(|err| panic!("{sql}: {err}"));
}

fn input(table: &str, filters: Vec<TableFilter>) -> FetchTablePreviewInput {
    FetchTablePreviewStatsRequest {
        session_id: String::new(),
        table: table.into(),
        schema: Some("public".into()),
        database: None,
        filters,
        code_filter: None,
        execution_id: None,
        count_budget_ms: None,
    }
    .into_input()
}

fn starts(column: &str, value: &str) -> TableFilter {
    TableFilter {
        column: column.into(),
        op: "starts".into(),
        value: value.into(),
    }
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn count_estimate_and_value_completion() {
    let session = connect().await;
    let session = &*session;
    write(session, "DROP TABLE IF EXISTS mq_eg_count").await;
    write(
        session,
        "CREATE TABLE mq_eg_count AS
         SELECT g AS id, 'user' || g || '@example.com' AS email
         FROM generate_series(1, 20000) g",
    )
    .await;
    write(session, "ANALYZE mq_eg_count").await;

    let exact = session
        .fetch_table_preview_stats(&input("mq_eg_count", Vec::new()))
        .await
        .expect("stats");
    assert!(exact.total_known);
    assert_eq!(exact.total_rows, 20_000);

    let estimate = session
        .estimate_table_rows(&input("mq_eg_count", Vec::new()))
        .await
        .expect("estimate")
        .expect("planner rows");
    assert!((18_000..=22_000).contains(&estimate), "{estimate}");
    let narrow = session
        .estimate_table_rows(&input("mq_eg_count", vec![starts("email", "user1999")]))
        .await
        .expect("filtered estimate")
        .expect("planner rows");
    assert!(narrow < estimate, "{narrow} < {estimate}");

    // A skipped count (control cancelled up front) still answers, unknown.
    let skipped = input("mq_eg_count", Vec::new());
    skipped.control.cancel();
    let stats = session
        .fetch_table_preview_stats(&skipped)
        .await
        .expect("stats");
    assert!(!stats.total_known);

    // Completion: a prefix past the first 1000 distinct values still matches.
    let values = session
        .distinct_column_values(&DistinctColumnValuesInput::from(
            DistinctColumnValuesRequest {
                session_id: String::new(),
                table: "mq_eg_count".into(),
                schema: Some("public".into()),
                database: None,
                column: "email".into(),
                limit: 20,
                prefix: Some("USER1999".into()),
            },
        ))
        .await
        .expect("distinct");
    assert!(!values.is_empty());
    assert!(values
        .iter()
        .all(|v| v.as_str().is_some_and(|s| s.starts_with("user1999"))));
    write(session, "DROP TABLE mq_eg_count").await;
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn a_slow_count_stops_when_cancelled() {
    let session = connect().await;
    let session = &*session;
    write(session, "DROP VIEW IF EXISTS mq_eg_slow").await;
    write(
        session,
        "CREATE VIEW mq_eg_slow AS
         SELECT g FROM generate_series(1, 20) g WHERE pg_sleep(0.5) IS NOT NULL",
    )
    .await;
    let slow = input("mq_eg_slow", Vec::new());
    let control: Arc<QueryControl> = Arc::clone(&slow.control);
    let started = Instant::now();
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        control.cancel();
    };
    let (stats, ()) = tokio::join!(session.fetch_table_preview_stats(&slow), cancel);
    let stats = stats.expect("stats");
    assert!(!stats.total_known);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    // The pool still serves the next request.
    write(session, "DROP VIEW mq_eg_slow").await;
}
