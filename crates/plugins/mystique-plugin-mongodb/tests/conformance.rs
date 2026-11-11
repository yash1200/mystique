//! The plugin conformance suite against a live MongoDB. The server comes from
//! `MYSTIQUE_MONGO_URI` (default: `admin:123@127.0.0.1:27017`, database
//! `mystique`). The kit's pinned-transaction check needs a SQL dialect, so it
//! is skipped here. Mongo transactions are covered by `tests/pinned.rs`, which
//! needs a replica set (`MYSTIQUE_MONGO_RS_URI`) and is not in the `live` job.
//!
//! `cargo test -p mystique-plugin-mongodb --test conformance -- --ignored`

use mystique_conformance::{env_or, run, Fixture, ROWS, TABLE};
use mystique_connector_api::{ConnectionConfig, PoolConfig};
use mystique_plugin_mongodb::MongoConnector;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs MongoDB (MYSTIQUE_MONGO_URI)"]
async fn mongodb_passes_the_conformance_suite() {
    let uri = env_or(
        "MYSTIQUE_MONGO_URI",
        "mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin",
    );
    let docs: Vec<String> = (1..=ROWS)
        .map(|i| format!("{{ _id: {i}, name: \"row {i}\" }}"))
        .collect();
    let fixture = Fixture {
        config: ConnectionConfig {
            engine: "mongodb".into(),
            plugin_config: json!({ "uri": uri, "database": "mystique" }),
            password: None,
            pool: PoolConfig {
                max_size: 4,
                min_idle: 0,
                acquire_timeout_ms: 5_000,
                idle_timeout_secs: 30,
            },
        },
        database: Some("mystique".into()),
        schema: None,
        key: "_id".into(),
        setup: vec![
            format!("db.{TABLE}.deleteMany({{}})"),
            format!("db.{TABLE}.insertMany([{}])", docs.join(", ")),
        ],
        // The executor cannot drop; an empty collection is left behind.
        teardown: vec![format!("db.{TABLE}.deleteMany({{}})")],
        count: format!("db.{TABLE}.countDocuments({{}})"),
        insert: format!(
            "db.{TABLE}.insertOne({{ _id: {}, name: \"extra\" }})",
            ROWS + 1
        ),
        // Server-side JavaScript (`$where: sleep`) is off on current servers.
        slow: None,
        dialect: None,
        collection: Some(TABLE.into()),
    };
    let report = run(&MongoConnector, &fixture).await;
    println!("{report}");
    report.assert_passed();
}
