//! Pinned (manual-commit) tabs run every call inside the client session.
//!
//! Needs a replica set, e.g. `mongod --replSet rs0` then `rs.initiate()`.
//! Override the URI with `MYSTIQUE_MONGO_RS_URI`.

use mystique_connector_api::{
    ConnectionConfig, ConnectorSession, DatabaseConnector, PinnedSession, PlannedStatement,
    PoolConfig, QueryBatchResult, QueryControl, QueryMode, SqlRange, StatementKind, ValidatedBatch,
    ValidatedQuery,
};
use mystique_plugin_mongodb::MongoConnector;
use serde_json::json;

const COLLECTION: &str = "s4_pin_probe";

fn rs_config() -> ConnectionConfig {
    let uri = std::env::var("MYSTIQUE_MONGO_RS_URI").unwrap_or_else(|_| {
        "mongodb://127.0.0.1:27017/mystique?replicaSet=rs0&directConnection=true".into()
    });
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

fn batch(text: &str) -> ValidatedBatch {
    ValidatedBatch {
        statements: vec![PlannedStatement {
            index: 0,
            sql: text.into(),
            sql_range: SqlRange {
                start: 0,
                end: u32::try_from(text.len()).unwrap_or(u32::MAX),
            },
            kind: StatementKind::Write,
            returns_rows: true,
            returns_count: false,
            cursor: false,
            multi_set: false,
            txn_boundary: false,
            explain_executes: false,
            params: Vec::new(),
        }],
        mode: QueryMode::Write,
        max_rows: 100,
        timeout_ms: 10_000,
        explain_executes: false,
        collection: Some(COLLECTION.into()),
        database: Some("mystique".into()),
        control: QueryControl::new(),
        progress: None,
    }
}

async fn run(pin: &mut Box<dyn PinnedSession + Send>, text: &str) -> QueryBatchResult {
    pin.execute(&batch(text))
        .await
        .unwrap_or_else(|err| panic!("{text}: {err}"))
}

/// Run outside any transaction.
async fn outside(
    session: &dyn ConnectorSession,
    text: &str,
) -> mystique_connector_api::ConnectorResult<mystique_connector_api::QueryResult> {
    session
        .execute_query(&ValidatedQuery {
            text: text.into(),
            mode: QueryMode::Write,
            max_rows: 10,
            timeout_ms: 10_000,
            collection: Some(COLLECTION.into()),
            database: Some("mystique".into()),
            control: QueryControl::new(),
            params: Vec::new(),
        })
        .await
}

async fn committed(session: &dyn ConnectorSession, filter: &str) -> u64 {
    let result = outside(
        session,
        &format!("db.{COLLECTION}.countDocuments({filter})"),
    )
    .await
    .expect("count");
    result.rows[0][0].as_u64().expect("count cell")
}

fn inner_count(ran: &QueryBatchResult) -> u64 {
    ran.results[0].result.as_ref().expect("result").rows[0][0]
        .as_u64()
        .expect("count cell")
}

#[tokio::test]
#[ignore = "needs a local MongoDB replica set"]
async fn rollback_discards_and_commit_keeps_pinned_writes() {
    let session = MongoConnector.connect(&rs_config()).await.expect("connect");
    let session = session.as_ref();
    // The collection must exist before a transaction writes to it on older servers.
    let _ = outside(session, &format!("db.{COLLECTION}.deleteMany({{}})")).await;
    outside(
        session,
        &format!("db.{COLLECTION}.insertOne({{ seed: 1 }})"),
    )
    .await
    .expect("seed");

    let mut pin = session.begin_pinned().await.expect("pin");
    run(
        &mut pin,
        &format!("db.{COLLECTION}.insertOne({{ k: 'rolled' }})"),
    )
    .await;
    assert!(pin.in_transaction());
    // Reads inside the transaction see the write; reads outside do not.
    let inside = run(
        &mut pin,
        &format!("db.{COLLECTION}.countDocuments({{ k: 'rolled' }})"),
    )
    .await;
    assert_eq!(inner_count(&inside), 1);
    let found = run(
        &mut pin,
        &format!("db.{COLLECTION}.find({{ k: 'rolled' }})"),
    )
    .await;
    assert_eq!(
        found.results[0].result.as_ref().map(|r| r.row_count),
        Some(1)
    );
    assert_eq!(committed(session, "{ k: 'rolled' }").await, 0);

    // Listing commands are refused instead of running outside the transaction.
    let listed = pin.execute(&batch("show collections")).await;
    assert!(listed.is_err(), "{listed:?}");

    pin.rollback().await.expect("rollback");
    assert_eq!(committed(session, "{ k: 'rolled' }").await, 0);

    run(
        &mut pin,
        &format!("db.{COLLECTION}.insertOne({{ k: 'kept' }})"),
    )
    .await;
    assert_eq!(committed(session, "{ k: 'kept' }").await, 0);
    pin.commit().await.expect("commit");
    assert_eq!(committed(session, "{ k: 'kept' }").await, 1);

    drop(pin);
    let _ = outside(session, &format!("db.{COLLECTION}.deleteMany({{}})")).await;
    session.close().await.ok();
}
