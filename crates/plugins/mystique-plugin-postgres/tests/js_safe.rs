//! Live check that int8, numeric, and the formerly-NULL types survive a read.
//!
//! `cargo test -p mystique-plugin-postgres --test js_safe -- --ignored`

use mystique_connector_api::{
    ConnectionConfig, DatabaseConnector, PoolConfig, QueryControl, QueryMode, ValidatedQuery,
};
use mystique_plugin_postgres::PostgresConnector;
use serde_json::json;

fn query(text: &str) -> ValidatedQuery {
    ValidatedQuery {
        text: text.into(),
        mode: QueryMode::Read,
        max_rows: 10,
        timeout_ms: 5_000,
        collection: None,
        database: None,
        control: QueryControl::new(),
        params: Vec::new(),
    }
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn int8_numeric_and_special_types_round_trip() {
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

    let result = session
        .execute_query(&query(
            "SELECT 9223372036854775807::int8, \
             9007199254740993::int8, \
             numeric '12345678901234567890.25', \
             32767::int2, \
             1.5::float4, \
             ARRAY[1]::int4[], \
             '127.0.0.1'::inet, \
             interval '1 day'",
        ))
        .await
        .expect("select");
    let row = result.rows.first().expect("row");
    assert_eq!(row[0], json!("9223372036854775807"));
    assert_eq!(row[1], json!("9007199254740993"));
    assert_eq!(row[2], json!("12345678901234567890.25"));
    assert_eq!(row[3], json!(32767));
    assert_eq!(row[4], json!(1.5));
    assert_eq!(row[5], json!([1]));
    assert_eq!(row[6], json!("127.0.0.1"));
    assert_eq!(row[7], json!("1 day"));
    assert_eq!(
        result.columns.get(1).and_then(|c| c.data_type.as_deref()),
        Some("int8")
    );
}
