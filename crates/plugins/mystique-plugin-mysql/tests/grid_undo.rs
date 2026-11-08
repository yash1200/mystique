//! Live grid save + rollback against MySQL.
//!
//! `cargo test -p mystique-plugin-mysql --test grid_undo -- --ignored`

use mystique_connector_api::{
    ApplyTableEditsInput, ConnectionConfig, ConnectorSession, DatabaseConnector, PoolConfig,
    QueryControl, QueryMode, TableEditColumn, TableRowDelete, TableRowUpdate, ValidatedQuery,
};
use mystique_plugin_mysql::MysqlConnector;
use serde_json::{json, Value as JsonValue};

async fn session() -> Box<dyn ConnectorSession> {
    MysqlConnector
        .connect(&ConnectionConfig {
            engine: "mysql".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 3306,
                "database": "mystique",
                "username": "admin"
            }),
            password: Some("123".into()),
            pool: PoolConfig {
                max_size: 2,
                min_idle: 0,
                acquire_timeout_ms: 5_000,
                idle_timeout_secs: 30,
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

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn update_and_delete_roll_back_once() {
    let session = session().await;
    let s = session.as_ref();
    run(s, "DROP TABLE IF EXISTS s5_grid_undo", QueryMode::Write).await;
    run(
        s,
        "CREATE TABLE s5_grid_undo (id int PRIMARY KEY, n int, at datetime(3))",
        QueryMode::Write,
    )
    .await;
    run(
        s,
        "INSERT INTO s5_grid_undo VALUES (1, 10, '2026-01-02 03:04:05.678'), (2, 20, NULL)",
        QueryMode::Write,
    )
    .await;
    let input = ApplyTableEditsInput {
        table: "s5_grid_undo".into(),
        schema: None,
        database: Some("mystique".into()),
        columns: vec![
            column("id", "int"),
            column("n", "int"),
            column("at", "datetime"),
        ],
        updates: vec![TableRowUpdate {
            original: [("id".into(), json!(1)), ("n".into(), json!(10))]
                .into_iter()
                .collect(),
            values: [("n".into(), json!(11))].into_iter().collect(),
        }],
        inserts: Vec::new(),
        deletes: vec![TableRowDelete {
            original: [
                ("id".into(), json!(2)),
                ("n".into(), json!(20)),
                ("at".into(), JsonValue::Null),
            ]
            .into_iter()
            .collect(),
        }],
        dry_run: false,
    };
    let saved = s.apply_table_edits(&input).await.expect("save");
    let undo = saved.undo.expect("undo");
    s.apply_grid_undo(&undo).await.expect("rollback");
    // Write mode: Read mode on MySQL is R-01 (A-02), not this test.
    let rows = run(
        s,
        "SELECT CONCAT(id, ':', n) FROM s5_grid_undo ORDER BY id",
        QueryMode::Write,
    )
    .await;
    assert_eq!(rows, vec![vec![json!("1:10")], vec![json!("2:20")]]);
    let again = s.apply_grid_undo(&undo).await.expect_err("second rollback");
    assert!(again.to_string().contains("rollback_stale"), "{again}");
    run(s, "DROP TABLE s5_grid_undo", QueryMode::Write).await;
}
