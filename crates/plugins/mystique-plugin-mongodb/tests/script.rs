//! Run all on a buffer of several shell statements, against a live server:
//! each piece the policy cuts must be one the executor runs. Core's batch
//! loop (validate, guard, progress, stop on error) is tested with doubles in
//! `mystique-core`.

use mystique_connector_api::{
    ConnectionConfig, ConnectorSession, DatabaseConnector, PoolConfig, QueryControl, QueryMode,
    ValidatedQuery,
};
use mystique_plugin_mongodb::{query_policy, MongoConnector};
use serde_json::json;

const COLLECTION: &str = "s5_script_probe";

fn config() -> ConnectionConfig {
    let uri = std::env::var("MYSTIQUE_MONGO_URI")
        .unwrap_or_else(|_| "mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin".into());
    ConnectionConfig {
        engine: "mongodb".into(),
        plugin_config: json!({ "uri": uri, "database": "mystique" }),
        password: None,
        pool: PoolConfig {
            max_size: 2,
            min_idle: 0,
            acquire_timeout_ms: 5000,
            idle_timeout_secs: 30,
        },
    }
}

async fn run(
    session: &dyn ConnectorSession,
    text: &str,
) -> mystique_connector_api::ConnectorResult<mystique_connector_api::QueryResult> {
    session
        .execute_query(&ValidatedQuery {
            text: text.into(),
            mode: QueryMode::Write,
            max_rows: 100,
            timeout_ms: 10_000,
            collection: None,
            database: Some("mystique".into()),
            control: QueryControl::new(),
            params: Vec::new(),
        })
        .await
}

#[tokio::test]
#[ignore = "needs local MongoDB"]
async fn every_split_statement_runs_on_the_server() {
    let session = MongoConnector.connect(&config()).await.expect("connect");
    let session = session.as_ref();
    let script = format!(
        "db.{COLLECTION}.deleteMany({{}})\n\
         db.{COLLECTION}\n  .insertOne({{ k: \"a;b\" }});\n\
         // a comment between statements\n\
         db.getSiblingDB(\"mystique\")\n  .{COLLECTION}\n  .insertMany([\n    {{ k: 1 }},\n    {{ k: 2 }}\n  ])\n\
         db.{COLLECTION}.countDocuments({{}})"
    );
    let policy = query_policy();
    let ranges = policy.split(&script);
    assert_eq!(ranges.len(), 4, "{ranges:?}");
    let mut last = None;
    for range in ranges {
        let text = &script[range];
        policy
            .validate(text, QueryMode::Write)
            .unwrap_or_else(|err| panic!("{text}: {err}"));
        last = Some(
            run(session, text)
                .await
                .unwrap_or_else(|err| panic!("{text}: {err}")),
        );
    }
    let count = last.expect("count").rows[0][0].as_u64();
    assert_eq!(count, Some(3));
    let _ = run(session, &format!("db.{COLLECTION}.deleteMany({{}})")).await;
    session.close().await.ok();
}
