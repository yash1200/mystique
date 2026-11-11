//! The plugin conformance suite on a temp SQLite file. Needs no server, so it
//! runs in the ordinary `cargo test` on every push.

use mystique_conformance::{run, Fixture, ROWS, TABLE};
use mystique_connector_api::{ConnectionConfig, PoolConfig};
use mystique_plugin_sqlite::SqliteConnector;
use mystique_sql::SqlDialect;
use serde_json::json;

fn temp_db() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "mystique-conformance-{}-{nanos}.sqlite",
        std::process::id()
    ))
}

#[tokio::test(flavor = "multi_thread")]
async fn sqlite_passes_the_conformance_suite() {
    let path = temp_db();
    let fixture = Fixture {
        config: ConnectionConfig {
            engine: "sqlite".into(),
            plugin_config: json!({ "path": path.to_string_lossy() }),
            password: None,
            pool: PoolConfig {
                max_size: 4,
                min_idle: 0,
                acquire_timeout_ms: 5_000,
                idle_timeout_secs: 30,
            },
        },
        database: Some("main".into()),
        schema: None,
        key: "id".into(),
        setup: vec![
            format!("DROP TABLE IF EXISTS {TABLE}"),
            format!("CREATE TABLE {TABLE} (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"),
            format!(
                "INSERT INTO {TABLE} (id, name) \
                 WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {ROWS}) \
                 SELECT i, 'row ' || i FROM n"
            ),
        ],
        teardown: vec![format!("DROP TABLE IF EXISTS {TABLE}")],
        count: format!("SELECT COUNT(*) FROM {TABLE}"),
        insert: format!(
            "INSERT INTO {TABLE} (id, name) VALUES ({}, 'extra')",
            ROWS + 1
        ),
        slow: Some(
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 1000000000) \
             SELECT COUNT(*) FROM c"
                .into(),
        ),
        dialect: Some(SqlDialect::Sqlite),
        collection: None,
    };
    let report = run(&SqliteConnector, &fixture).await;
    println!("{report}");
    let _ = std::fs::remove_file(&path);
    report.assert_passed();
}
