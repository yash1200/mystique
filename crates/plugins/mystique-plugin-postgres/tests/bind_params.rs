//! Live checks for bound parameters and Postgres operators that look like them.
//!
//! `cargo test -p mystique-plugin-postgres --test bind_params -- --ignored`
//!
//! Uses the local `postgres` database and drops its `s3_` scratch table.

use mystique_connector_api::{
    gate_sql_batch_in, ConnectionConfig, ConnectorSession, DatabaseConnector, PoolConfig,
    QueryBatchResult, QueryControl, QueryMode, ValidatedBatch,
};
use mystique_plugin_postgres::PostgresConnector;
use mystique_sql::SqlDialect;
use serde_json::{json, Value};

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
                max_size: 1,
                min_idle: 0,
                acquire_timeout_ms: 5_000,
                idle_timeout_secs: 10,
            },
        })
        .await
        .expect("connect")
}

/// Same path as the editor, the agent and MCP: gate, then the plugin.
async fn run(
    session: &dyn ConnectorSession,
    text: &str,
    mode: QueryMode,
    params: &[Value],
) -> QueryBatchResult {
    let plan = gate_sql_batch_in(text, SqlDialect::Postgres, mode, false, params)
        .unwrap_or_else(|err| panic!("gate refused {text:?}: {err}"));
    let batch = ValidatedBatch {
        statements: plan.statements,
        mode,
        max_rows: 50,
        timeout_ms: 5_000,
        explain_executes: plan.explain_executes,
        collection: None,
        database: None,
        control: QueryControl::new(),
        progress: None,
    };
    let result = session.execute_batch(&batch).await.expect("batch");
    for stmt in &result.results {
        assert!(stmt.error.is_none(), "{text:?} failed: {:?}", stmt.error);
    }
    result
}

fn rows(result: &QueryBatchResult) -> Vec<Vec<Value>> {
    result
        .results
        .iter()
        .rev()
        .find_map(|stmt| stmt.result.as_ref())
        .map(|res| res.rows.clone())
        .unwrap_or_default()
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn binds_reach_typed_columns_as_text() {
    let session = connect().await;
    run(
        &*session,
        "DROP TABLE IF EXISTS s3_bind_t; CREATE TABLE s3_bind_t \
         (i int4, u uuid, ts timestamptz, n numeric(10,2), t text, b bool)",
        QueryMode::Write,
        &[],
    )
    .await;

    let uuid = "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11";
    let inserted = run(
        &*session,
        "INSERT INTO s3_bind_t VALUES (:i, :u, :ts, :n, :t, :b), (:i2, NULL, NULL, NULL, :none, NULL)",
        QueryMode::Write,
        &[
            json!(7),
            json!(uuid),
            json!("2024-01-02T03:04:05Z"),
            json!(12.5),
            json!("hello"),
            json!(true),
            json!(8),
            Value::Null,
        ],
    )
    .await;
    assert_eq!(inserted.results[0].affected_rows, Some(2));

    let found = run(
        &*session,
        "SELECT t FROM s3_bind_t WHERE i = :i AND u = :u AND ts = :ts AND n = :n AND t = :t AND b = :b",
        QueryMode::Read,
        &[
            json!(7),
            json!(uuid),
            json!("2024-01-02 03:04:05+00"),
            json!(12.5),
            json!("hello"),
            json!(true),
        ],
    )
    .await;
    assert_eq!(rows(&found), vec![vec![json!("hello")]]);

    let null = run(
        &*session,
        "SELECT i FROM s3_bind_t WHERE t IS NOT DISTINCT FROM :t",
        QueryMode::Read,
        &[Value::Null],
    )
    .await;
    assert_eq!(rows(&null), vec![vec![json!(8)]]);

    // `$n` the user typed, and a value whose type the server infers as text.
    let dollar = run(
        &*session,
        "SELECT $1::int + 1, $2",
        QueryMode::Read,
        &[json!(41), json!("as text")],
    )
    .await;
    assert_eq!(rows(&dollar), vec![vec![json!(42), json!("as text")]]);

    run(&*session, "DROP TABLE s3_bind_t", QueryMode::Write, &[]).await;
    let _ = session.close().await;
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn jsonb_question_operators_and_array_slices_need_no_params() {
    let session = connect().await;
    let jsonb = run(
        &*session,
        "SELECT '{\"a\":1}'::jsonb ? 'a', '{\"a\":1}'::jsonb ?| array['a','z'], \
         '{\"a\":1}'::jsonb ?& array['a','z']",
        QueryMode::Read,
        &[],
    )
    .await;
    assert_eq!(
        rows(&jsonb),
        vec![vec![json!(true), json!(true), json!(false)]]
    );

    let slice = run(
        &*session,
        "WITH a AS (SELECT ARRAY[1,2,3,4] AS arr, 2 AS lo, 3 AS hi) \
         SELECT arr[lo:hi]::text, arr[:lo]::text, arr[hi:]::text, arr[lo]::text FROM a",
        QueryMode::Read,
        &[],
    )
    .await;
    assert_eq!(
        rows(&slice),
        vec![vec![
            json!("{2,3}"),
            json!("{1,2}"),
            json!("{3,4}"),
            json!("2")
        ]]
    );

    // A placeholder in an array constructor and a subquery inside a subscript is still a bind.
    let bound = run(
        &*session,
        "SELECT (ARRAY[:a::int, :b::int])[(SELECT :i::int)]",
        QueryMode::Read,
        &[json!(10), json!(20), json!(2)],
    )
    .await;
    assert_eq!(rows(&bound), vec![vec![json!(20)]]);
    let _ = session.close().await;
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn bound_statement_error_points_at_the_source_text() {
    let session = connect().await;
    let text = "SELECT 1;\nSELECT :long_name, nosuchcol";
    let plan = gate_sql_batch_in(
        text,
        SqlDialect::Postgres,
        QueryMode::Read,
        false,
        &[json!(1)],
    )
    .expect("gate");
    let batch = ValidatedBatch {
        statements: plan.statements,
        mode: QueryMode::Read,
        max_rows: 50,
        timeout_ms: 5_000,
        explain_executes: false,
        collection: None,
        database: None,
        control: QueryControl::new(),
        progress: None,
    };
    let mut result = session.execute_batch(&batch).await.expect("batch");
    // What core does with every batch before the editor sees it.
    mystique_connector_api::place_error_positions(text, SqlDialect::Postgres, &mut result);
    let failed = result.results.last().expect("second statement");
    assert!(failed.error.is_some());
    let position = failed.error_position.as_ref().expect("position");
    assert_eq!(
        &text[position.offset as usize..],
        "nosuchcol",
        "offset drifted: {position:?}"
    );
    assert_eq!((position.line, position.column), (2, 20));
    let _ = session.close().await;
}
