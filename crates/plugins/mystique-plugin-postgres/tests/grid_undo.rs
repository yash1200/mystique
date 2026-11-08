//! Live grid save + rollback against Postgres.
//!
//! `cargo test -p mystique-plugin-postgres --test grid_undo -- --ignored`

use mystique_connector_api::{
    ApplyTableEditsInput, ConnectionConfig, ConnectorSession, DatabaseConnector, PoolConfig,
    QueryControl, QueryMode, TableEditColumn, TableRowInsert, TableRowUpdate, ValidatedQuery,
};
use mystique_plugin_postgres::PostgresConnector;
use serde_json::{json, Value as JsonValue};

async fn session() -> Box<dyn ConnectorSession> {
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
                max_size: 2,
                min_idle: 0,
                acquire_timeout_ms: 5_000,
                idle_timeout_secs: 10,
            },
        })
        .await
        .expect("connect")
}

async fn run(session: &dyn ConnectorSession, text: &str, mode: QueryMode) -> Vec<Vec<JsonValue>> {
    let query = ValidatedQuery {
        text: text.into(),
        mode,
        max_rows: 10,
        timeout_ms: 5_000,
        collection: None,
        database: None,
        control: QueryControl::new(),
        params: Vec::new(),
    };
    session.execute_query(&query).await.expect("sql").rows
}

fn column(name: &str, data_type: &str) -> TableEditColumn {
    TableEditColumn {
        name: name.into(),
        data_type: data_type.into(),
        nullable: true,
        pk: false,
    }
}

fn edits(table: &str) -> ApplyTableEditsInput {
    ApplyTableEditsInput {
        table: table.into(),
        schema: Some("public".into()),
        database: None,
        columns: vec![
            column("id", "int4"),
            column("n", "int4"),
            column("at", "timestamptz"),
            column("doc", "json"),
        ],
        updates: Vec::new(),
        inserts: Vec::new(),
        deletes: Vec::new(),
        dry_run: false,
    }
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn update_rolls_back_once_and_then_is_stale() {
    let session = session().await;
    let s = session.as_ref();
    run(
        s,
        "DROP TABLE IF EXISTS s5_grid_undo; \
         CREATE TABLE s5_grid_undo (id int PRIMARY KEY, n int, at timestamptz DEFAULT now(), doc json); \
         INSERT INTO s5_grid_undo (id, n, doc) VALUES (1, 10, '{\"a\":1}')",
        QueryMode::Write,
    )
    .await;
    let mut input = edits("s5_grid_undo");
    input.updates = vec![TableRowUpdate {
        original: [("id".into(), json!(1)), ("n".into(), json!(10))]
            .into_iter()
            .collect(),
        values: [("n".into(), json!(11))].into_iter().collect(),
    }];
    let saved = s.apply_table_edits(&input).await.expect("save");
    let undo = saved.undo.expect("undo");
    s.apply_grid_undo(&undo).await.expect("rollback");
    let rows = run(
        s,
        "SELECT n FROM s5_grid_undo WHERE id = 1",
        QueryMode::Read,
    )
    .await;
    assert_eq!(rows, vec![vec![json!(10)]]);
    let again = s.apply_grid_undo(&undo).await.expect_err("second rollback");
    assert!(again.to_string().contains("rollback_stale"), "{again}");
    run(s, "DROP TABLE s5_grid_undo", QueryMode::Write).await;
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn a_row_that_cannot_be_read_back_saves_without_an_undo() {
    let session = session().await;
    let s = session.as_ref();
    run(
        s,
        "DROP TABLE IF EXISTS s5_grid_undo_json; \
         CREATE TABLE s5_grid_undo_json (id serial PRIMARY KEY, n int, at timestamptz, doc json)",
        QueryMode::Write,
    )
    .await;
    // `json = json` has no operator, so the read-back fails inside a savepoint.
    let mut input = edits("s5_grid_undo_json");
    input.inserts = vec![TableRowInsert {
        values: [("n".into(), json!(5)), ("doc".into(), json!("{\"a\":1}"))]
            .into_iter()
            .collect(),
    }];
    let saved = s.apply_table_edits(&input).await.expect("save");
    assert!(saved.undo.is_none());
    let rows = run(s, "SELECT n FROM s5_grid_undo_json", QueryMode::Read).await;
    assert_eq!(rows, vec![vec![json!(5)]]);
    run(s, "DROP TABLE s5_grid_undo_json", QueryMode::Write).await;
}
