//! Optional live catalog checks against a local Mongo (skipped if unreachable).

use mongodb::bson::doc;
use mongodb::Client;
use mystique_connector_api::{ConnectorSession, QueryMode, ValidatedQuery};

use super::MongoSession;

#[tokio::test]
#[ignore = "needs local MongoDB"]
async fn list_databases_and_find_users_on_mystique() {
    let uri = "mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin";
    let client = match Client::with_uri_str(uri).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skip: {e}");
            return;
        }
    };
    if client
        .database("admin")
        .run_command(doc! { "ping": 1 })
        .await
        .is_err()
    {
        eprintln!("skip: mongo not reachable");
        return;
    }
    let session = MongoSession::new(client, 2, Some("mystique".into()));
    let dbs = session
        .execute_query(&ValidatedQuery {
            text: "listDatabases".into(),
            mode: QueryMode::Read,
            max_rows: 50,
            timeout_ms: 5000,
            collection: None,
            database: None,
        })
        .await
        .expect("listDatabases");
    let names: Vec<String> = dbs
        .rows
        .iter()
        .map(|r| r[0].as_str().unwrap_or("").to_string())
        .collect();
    assert!(names.iter().any(|n| n == "mystique"), "names={names:?}");

    let colls = session
        .execute_query(&ValidatedQuery {
            text: "listCollections".into(),
            mode: QueryMode::Read,
            max_rows: 50,
            timeout_ms: 5000,
            collection: None,
            database: Some("mystique".into()),
        })
        .await
        .expect("listCollections");
    assert!(
        colls.row_count >= 6,
        "expected seed collections, got {}",
        colls.row_count
    );

    let users = session
        .execute_query(&ValidatedQuery {
            text: "{}".into(),
            mode: QueryMode::Read,
            max_rows: 10,
            timeout_ms: 5000,
            collection: Some("users".into()),
            database: Some("mystique".into()),
        })
        .await
        .expect("find users");
    assert!(users.row_count >= 1, "expected user docs");
    assert!(users.columns.iter().any(|c| c.name == "_id"));
    println!(
        "ok: dbs={:?} collections={} sample_users={}",
        names, colls.row_count, users.row_count
    );
}
