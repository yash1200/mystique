//! The plugin conformance suite, plus the open object kinds, against a live
//! Postgres. The server comes from `MYSTIQUE_PG_{HOST,PORT,USER,PASSWORD,DATABASE}`
//! (default: 127.0.0.1:5432, `$USER`, no password, `postgres`).
//!
//! `cargo test -p mystique-plugin-postgres --test conformance -- --ignored`

use mystique_conformance::{env_or, run, Fixture, ROWS, TABLE};
use mystique_connector_api::{
    ConnectionConfig, ConnectorSession, DatabaseConnector, PoolConfig, QueryMode, TableAdminInput,
    TableAdminOp, TableObjectKind, ValidatedQuery,
};
use mystique_plugin_postgres::PostgresConnector;
use mystique_sql::SqlDialect;
use serde_json::json;

fn config() -> ConnectionConfig {
    let user = env_or("MYSTIQUE_PG_USER", &env_or("USER", "postgres"));
    let password = std::env::var("MYSTIQUE_PG_PASSWORD").ok();
    ConnectionConfig {
        engine: "postgres".into(),
        plugin_config: json!({
            "host": env_or("MYSTIQUE_PG_HOST", "127.0.0.1"),
            "port": env_or("MYSTIQUE_PG_PORT", "5432").parse::<u16>().unwrap_or(5432),
            "database": env_or("MYSTIQUE_PG_DATABASE", "postgres"),
            "username": user,
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
#[ignore = "needs Postgres (MYSTIQUE_PG_*)"]
async fn postgres_passes_the_conformance_suite() {
    let fixture = Fixture {
        config: config(),
        database: Some(env_or("MYSTIQUE_PG_DATABASE", "postgres")),
        schema: Some("public".into()),
        key: "id".into(),
        setup: vec![
            format!("DROP TABLE IF EXISTS public.{TABLE}"),
            format!("CREATE TABLE public.{TABLE} (id integer PRIMARY KEY, name text NOT NULL)"),
            format!(
                "INSERT INTO public.{TABLE} (id, name) \
                 SELECT i, 'row ' || i FROM generate_series(1, {ROWS}) AS i"
            ),
        ],
        teardown: vec![format!("DROP TABLE IF EXISTS public.{TABLE}")],
        count: format!("SELECT COUNT(*) FROM public.{TABLE}"),
        insert: format!(
            "INSERT INTO public.{TABLE} (id, name) VALUES ({}, 'extra')",
            ROWS + 1
        ),
        slow: Some("SELECT pg_sleep(10)".into()),
        dialect: Some(SqlDialect::Postgres),
        collection: None,
    };
    let report = run(&PostgresConnector, &fixture).await;
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
        schema: Some("mystique_kinds".into()),
        database: None,
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
#[ignore = "needs Postgres (MYSTIQUE_PG_*)"]
async fn postgres_lists_routines_sequences_matviews_and_types_with_definitions() {
    let session = PostgresConnector.connect(&config()).await.expect("connect");
    let s = session.as_ref();
    exec(s, "DROP SCHEMA IF EXISTS mystique_kinds CASCADE").await;
    exec(s, "CREATE SCHEMA mystique_kinds").await;
    exec(s, "CREATE TABLE mystique_kinds.t (id int PRIMARY KEY)").await;
    exec(
        s,
        "CREATE MATERIALIZED VIEW mystique_kinds.mv AS SELECT id FROM mystique_kinds.t",
    )
    .await;
    exec(s, "CREATE SEQUENCE mystique_kinds.seq START 7 INCREMENT 3").await;
    exec(
        s,
        "CREATE FUNCTION mystique_kinds.add(a integer, b integer) RETURNS integer \
         LANGUAGE sql IMMUTABLE AS 'SELECT a + b'",
    )
    .await;
    exec(
        s,
        "CREATE FUNCTION mystique_kinds.add(a text, b text) RETURNS text \
         LANGUAGE sql IMMUTABLE AS 'SELECT a || b'",
    )
    .await;
    exec(
        s,
        "CREATE PROCEDURE mystique_kinds.touch() LANGUAGE sql AS 'SELECT 1'",
    )
    .await;
    exec(
        s,
        "CREATE TYPE mystique_kinds.mood AS ENUM ('sad', 'o''k', 'happy')",
    )
    .await;
    exec(s, "CREATE TYPE mystique_kinds.pair AS (a int, b text)").await;
    exec(
        s,
        "CREATE DOMAIN mystique_kinds.pos AS integer NOT NULL DEFAULT 1 CHECK (VALUE > 0)",
    )
    .await;

    let dbs = s.list_catalog(None, true).await.expect("catalog");
    let mut listed: Vec<(String, String)> = dbs
        .iter()
        .flat_map(|d| d.objects.iter())
        .filter(|o| o.schema.as_deref() == Some("mystique_kinds"))
        .map(|o| (o.kind.clone(), o.name.clone()))
        .collect();
    listed.sort();
    let want: Vec<(String, String)> = [
        ("function", "add(a integer, b integer)"),
        ("function", "add(a text, b text)"),
        ("matview", "mv"),
        ("procedure", "touch()"),
        ("sequence", "seq"),
        ("table", "t"),
        ("type", "mood"),
        ("type", "pair"),
        ("type", "pos"),
    ]
    .iter()
    .map(|(k, n)| ((*k).to_string(), (*n).to_string()))
    .collect();
    assert_eq!(listed, want);

    let f = definition(s, "add(a text, b text)", "function").await;
    assert!(
        f.contains("CREATE OR REPLACE FUNCTION mystique_kinds.add(a text, b text)"),
        "{f}"
    );
    let p = definition(s, "touch()", "procedure").await;
    assert!(p.contains("PROCEDURE mystique_kinds.touch()"), "{p}");
    let seq = definition(s, "seq", "sequence").await;
    assert!(
        seq.contains("INCREMENT BY 3") && seq.contains("START WITH 7"),
        "{seq}"
    );
    let mv = definition(s, "mv", "matview").await;
    assert!(mv.starts_with("CREATE MATERIALIZED VIEW"), "{mv}");
    let mood = definition(s, "mood", "type").await;
    assert!(
        mood.contains("AS ENUM") && mood.contains("'o''k'"),
        "{mood}"
    );
    let pair = definition(s, "pair", "type").await;
    assert!(
        pair.contains("\"a\" integer") && pair.contains("\"b\" text"),
        "{pair}"
    );
    let pos = definition(s, "pos", "type").await;
    assert!(
        pos.contains("CREATE DOMAIN") && pos.contains("NOT NULL") && pos.contains("CHECK"),
        "{pos}"
    );

    exec(s, "DROP SCHEMA mystique_kinds CASCADE").await;
    s.close().await.expect("close");
}
