//! Live check that a capped Postgres fetch does not buffer the whole series.
//!
//! `cargo test -p mystique-plugin-postgres --test stream_rows -- --ignored`
//!
//! Memory of the call stays O(max_rows): the server is asked for `max_rows + 1`
//! via `FETCH`, then the cursor is closed. A billion-row `generate_series`
//! must return the cap and leave the session usable. It must not grow with
//! the series length.

use mystique_connector_api::{
    ConnectionConfig, DatabaseConnector, PoolConfig, QueryControl, QueryMode, ValidatedQuery,
};
use mystique_plugin_postgres::PostgresConnector;
use serde_json::json;

fn query(text: &str, max_rows: u32) -> ValidatedQuery {
    ValidatedQuery {
        text: text.into(),
        mode: QueryMode::Read,
        max_rows,
        timeout_ms: 3_000,
        collection: None,
        database: None,
        control: QueryControl::new(),
    }
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn generate_series_stops_at_max_rows_and_session_stays_usable() {
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

    let capped = session
        .execute_query(&query("SELECT generate_series(1, 1000000000)", 10))
        .await
        .expect("capped series");
    assert_eq!(capped.rows.len(), 10);
    assert!(capped.truncated, "the probe row should mark the cap");
    assert!(
        capped.duration_ms < 3_000,
        "fetch grew with the series: {} ms",
        capped.duration_ms
    );

    let mut write_cap = query("SELECT generate_series(1, 1000000000)", 10);
    write_cap.mode = QueryMode::Write;
    let write = session
        .execute_query(&write_cap)
        .await
        .expect("capped series outside the read-only transaction");
    assert_eq!(write.rows.len(), 10);
    assert!(write.truncated);

    let exact = session
        .execute_query(&query("SELECT generate_series(1, 10)", 10))
        .await
        .expect("exact series");
    assert_eq!(exact.rows.len(), 10);
    assert!(!exact.truncated);

    let follow = session
        .execute_query(&query("SELECT 1", 10))
        .await
        .expect("session still usable");
    assert_eq!(follow.rows.len(), 1);
}
