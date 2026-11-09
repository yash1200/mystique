//! Live checks for the grid's background count and value completion.
//!
//! `cargo test -p mystique-plugin-mysql --test grid_count -- --ignored`
//!
//! Uses the local `mystique` database. Scratch objects start with `mq_eg_`.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{local_config, write};
use mystique_connector_api::{
    ConnectorSession, DatabaseConnector, DistinctColumnValuesInput, DistinctColumnValuesRequest,
    FetchTablePreviewInput, FetchTablePreviewStatsRequest, QueryControl, TableFilter,
};
use mystique_plugin_mysql::MysqlConnector;

/// Three connections: the count, the `KILL QUERY`, and the next request.
async fn connect() -> Box<dyn ConnectorSession> {
    let mut config = local_config();
    config.pool.max_size = 3;
    MysqlConnector.connect(&config).await.expect("connect")
}

fn input(table: &str, filters: Vec<TableFilter>) -> FetchTablePreviewInput {
    FetchTablePreviewStatsRequest {
        session_id: String::new(),
        table: table.into(),
        schema: None,
        database: Some("mystique".into()),
        filters,
        code_filter: None,
        execution_id: None,
        count_budget_ms: None,
    }
    .into_input()
}

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn count_estimate_and_value_completion() {
    let session = connect().await;
    let session = &*session;
    write(session, "DROP TABLE IF EXISTS mq_eg_count").await;
    write(
        session,
        "CREATE TABLE mq_eg_count (id INT PRIMARY KEY, email VARCHAR(64) NOT NULL)",
    )
    .await;
    write(
        session,
        "INSERT INTO mq_eg_count
         WITH RECURSIVE seq (n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq WHERE n < 1000)
         SELECT n, CONCAT('user', n, '@example.com') FROM seq",
    )
    .await;
    write(session, "ANALYZE TABLE mq_eg_count").await;

    let exact = session
        .fetch_table_preview_stats(&input("mq_eg_count", Vec::new()))
        .await
        .expect("stats");
    assert!(exact.total_known);
    assert_eq!(exact.total_rows, 1_000);

    let estimate = session
        .estimate_table_rows(&input("mq_eg_count", Vec::new()))
        .await
        .expect("estimate")
        .expect("optimizer rows");
    assert!((500..=2_000).contains(&estimate), "{estimate}");

    let skipped = input("mq_eg_count", Vec::new());
    skipped.control.cancel();
    let stats = session
        .fetch_table_preview_stats(&skipped)
        .await
        .expect("stats");
    assert!(!stats.total_known);

    let values = session
        .distinct_column_values(&DistinctColumnValuesInput::from(
            DistinctColumnValuesRequest {
                session_id: String::new(),
                table: "mq_eg_count".into(),
                schema: None,
                database: Some("mystique".into()),
                column: "email".into(),
                limit: 20,
                prefix: Some("user99".into()),
            },
        ))
        .await
        .expect("distinct");
    assert!(!values.is_empty());
    assert!(values
        .iter()
        .all(|v| v.as_str().is_some_and(|s| s.starts_with("user99"))));
    write(session, "DROP TABLE mq_eg_count").await;
}

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn a_slow_count_stops_when_cancelled() {
    let session = connect().await;
    let session = &*session;
    write(session, "DROP VIEW IF EXISTS mq_eg_slow").await;
    write(
        session,
        "CREATE VIEW mq_eg_slow AS
         SELECT n FROM (SELECT 1 AS n UNION ALL SELECT 2 UNION ALL SELECT 3
                        UNION ALL SELECT 4 UNION ALL SELECT 5) s
         WHERE SLEEP(1) = 0",
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
        started.elapsed() < Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );
    write(session, "DROP VIEW mq_eg_slow").await;
}
