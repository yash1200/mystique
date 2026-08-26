use mystique_connector_api::{
    ConnectionConfig, DatabaseConnector, PoolConfig, QueryMode, ValidatedQuery,
};
use mystique_plugin_mysql::MysqlConnector;
use serde_json::json;

fn local_config() -> ConnectionConfig {
    ConnectionConfig {
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
            acquire_timeout_ms: 5000,
            idle_timeout_secs: 30,
        },
    }
}

#[tokio::test]
async fn list_tables_and_preview_users() {
    let session = MysqlConnector
        .connect(&local_config())
        .await
        .expect("connect");
    session.ping().await.expect("ping");

    let tables = session
        .execute_query(&ValidatedQuery {
            text: "SELECT table_schema, table_name, table_type FROM information_schema.tables WHERE table_schema NOT IN ('mysql','information_schema','performance_schema','sys') ORDER BY table_name".into(),
            mode: QueryMode::Read,
            max_rows: 100,
            timeout_ms: 10_000,
            collection: None,
            database: None,
        })
        .await
        .expect("list tables");
    assert!(
        tables.row_count >= 5,
        "expected seed tables, got {}",
        tables.row_count
    );
    let names: Vec<String> = tables
        .rows
        .iter()
        .filter_map(|r| r.get(1).and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();
    assert!(names.iter().any(|n| n == "users"), "names={names:?}");
    assert!(names.iter().any(|n| n == "orders"), "names={names:?}");

    let preview = session
        .execute_query(&ValidatedQuery {
            text: "SELECT * FROM `mystique`.`users` LIMIT 10".into(),
            mode: QueryMode::Read,
            max_rows: 10,
            timeout_ms: 10_000,
            collection: None,
            database: None,
        })
        .await
        .expect("preview");
    assert_eq!(preview.row_count, 10);
    assert!(!preview.columns.is_empty());
    session.close().await.expect("close");
}

#[tokio::test]
async fn information_schema_columns_and_fks() {
    let session = MysqlConnector
        .connect(&local_config())
        .await
        .expect("connect");
    let cols = session
        .execute_query(&ValidatedQuery {
            text: "SELECT table_name, column_name, column_type, is_nullable, column_key FROM information_schema.columns WHERE table_schema = 'mystique' ORDER BY table_name, ordinal_position".into(),
            mode: QueryMode::Read,
            max_rows: 5000,
            timeout_ms: 10_000,
            collection: None,
            database: None,
        })
        .await
        .expect("columns");
    assert!(cols.row_count >= 20, "got {}", cols.row_count);

    let fks = session
        .execute_query(&ValidatedQuery {
            text: "SELECT table_name, column_name, referenced_table_name, referenced_column_name FROM information_schema.key_column_usage WHERE table_schema = 'mystique' AND referenced_table_name IS NOT NULL".into(),
            mode: QueryMode::Read,
            max_rows: 100,
            timeout_ms: 10_000,
            collection: None,
            database: None,
        })
        .await
        .expect("fks");
    assert_eq!(fks.row_count, 4, "seed has 4 FKs");
    session.close().await.ok();
}
