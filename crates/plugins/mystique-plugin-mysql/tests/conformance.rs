//! The plugin conformance suite, plus routines and triggers, against a live
//! MySQL or MariaDB. The server comes from
//! `MYSTIQUE_MYSQL_{HOST,PORT,USER,PASSWORD,DATABASE}` (default:
//! 127.0.0.1:3306, `admin` / `123`, database `mystique`, which must exist).
//!
//! `cargo test -p mystique-plugin-mysql --test conformance -- --ignored`

use mystique_conformance::{env_or, run, Fixture, ROWS, TABLE};
use mystique_connector_api::{
    ConnectionConfig, ConnectorSession, DatabaseConnector, PoolConfig, QueryMode, TableAdminInput,
    TableAdminOp, TableObjectKind, ValidatedQuery,
};
use mystique_plugin_mysql::MysqlConnector;
use mystique_sql::SqlDialect;
use serde_json::json;

fn database() -> String {
    env_or("MYSTIQUE_MYSQL_DATABASE", "mystique")
}

fn config() -> ConnectionConfig {
    let password = match std::env::var("MYSTIQUE_MYSQL_PASSWORD") {
        Ok(p) if p.is_empty() => None,
        Ok(p) => Some(p),
        Err(_) => Some("123".into()),
    };
    ConnectionConfig {
        engine: "mysql".into(),
        plugin_config: json!({
            "host": env_or("MYSTIQUE_MYSQL_HOST", "127.0.0.1"),
            "port": env_or("MYSTIQUE_MYSQL_PORT", "3306").parse::<u16>().unwrap_or(3306),
            "database": database(),
            "username": env_or("MYSTIQUE_MYSQL_USER", "admin"),
            "sslMode": "disable"
        }),
        password,
        pool: PoolConfig {
            max_size: 4,
            min_idle: 0,
            acquire_timeout_ms: 5_000,
            idle_timeout_secs: 30,
        },
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs MySQL (MYSTIQUE_MYSQL_*)"]
async fn mysql_passes_the_conformance_suite() {
    let fixture = Fixture {
        config: config(),
        database: Some(database()),
        schema: None,
        key: "id".into(),
        setup: vec![
            format!("DROP TABLE IF EXISTS {TABLE}"),
            format!("CREATE TABLE {TABLE} (id INT PRIMARY KEY, name VARCHAR(64) NOT NULL)"),
            format!(
                "INSERT INTO {TABLE} (id, name) \
                 WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {ROWS}) \
                 SELECT i, CONCAT('row ', i) FROM n"
            ),
        ],
        teardown: vec![format!("DROP TABLE IF EXISTS {TABLE}")],
        count: format!("SELECT COUNT(*) FROM {TABLE}"),
        insert: format!(
            "INSERT INTO {TABLE} (id, name) VALUES ({}, 'extra')",
            ROWS + 1
        ),
        slow: Some("SELECT SLEEP(10)".into()),
        dialect: Some(SqlDialect::Mysql),
        collection: None,
    };
    let report = run(&MysqlConnector, &fixture).await;
    println!("{report}");
    report.assert_passed();
}

async fn exec(s: &dyn ConnectorSession, sql: &str) {
    s.execute_query(&ValidatedQuery::new(sql, QueryMode::Write, 10, 10_000))
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
}

async fn definition(s: &dyn ConnectorSession, name: &str, kind: &str) -> String {
    s.apply_table_admin(&TableAdminInput {
        op: TableAdminOp::CreateStatement,
        name: name.into(),
        schema: None,
        database: Some(database()),
        kind: TableObjectKind::Table,
        object_kind: kind.into(),
        new_name: None,
    })
    .await
    .unwrap_or_else(|e| panic!("{kind} {name}: {e}"))
    .statement
    .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs MySQL (MYSTIQUE_MYSQL_*)"]
async fn mysql_lists_routines_and_triggers_with_definitions() {
    let session = MysqlConnector.connect(&config()).await.expect("connect");
    let s = session.as_ref();
    for stmt in [
        "DROP TRIGGER IF EXISTS mk_touch",
        "DROP FUNCTION IF EXISTS mk_add",
        "DROP PROCEDURE IF EXISTS mk_noop",
        "DROP TABLE IF EXISTS mk_t",
        "CREATE TABLE mk_t (id INT PRIMARY KEY, n INT NOT NULL DEFAULT 0)",
        "CREATE FUNCTION mk_add(a INT, b INT) RETURNS INT DETERMINISTIC RETURN a + b",
        "CREATE PROCEDURE mk_noop() SELECT 1",
        "CREATE TRIGGER mk_touch BEFORE INSERT ON mk_t FOR EACH ROW SET NEW.n = NEW.n + 1",
    ] {
        exec(s, stmt).await;
    }
    let dbs = s
        .list_catalog(Some(&database()), true)
        .await
        .expect("catalog");
    let mut listed: Vec<(String, String)> = dbs
        .iter()
        .filter(|d| d.name == database())
        .flat_map(|d| d.objects.iter())
        .filter(|o| o.name.starts_with("mk_"))
        .map(|o| (o.kind.clone(), o.name.clone()))
        .collect();
    listed.sort();
    assert_eq!(
        listed,
        [
            ("function".to_string(), "mk_add".to_string()),
            ("procedure".to_string(), "mk_noop".to_string()),
            ("table".to_string(), "mk_t".to_string()),
            ("trigger".to_string(), "mk_touch".to_string()),
        ]
    );
    let f = definition(s, "mk_add", "function").await;
    assert!(f.contains("FUNCTION `mk_add`"), "{f}");
    let p = definition(s, "mk_noop", "procedure").await;
    assert!(p.contains("PROCEDURE `mk_noop`"), "{p}");
    let t = definition(s, "mk_touch", "trigger").await;
    assert!(t.contains("TRIGGER `mk_touch`"), "{t}");
    for stmt in [
        "DROP TRIGGER mk_touch",
        "DROP FUNCTION mk_add",
        "DROP PROCEDURE mk_noop",
        "DROP TABLE mk_t",
    ] {
        exec(s, stmt).await;
    }
    s.close().await.expect("close");
}
