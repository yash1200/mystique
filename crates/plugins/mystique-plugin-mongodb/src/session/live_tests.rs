//! Optional live catalog checks against a local Mongo (skipped if unreachable).

use mongodb::bson::doc;
use mongodb::Client;
use mystique_connector_api::{ConnectorSession, QueryControl, QueryMode, ValidatedQuery};

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
            control: QueryControl::new(),
            params: Vec::new(),
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
            control: QueryControl::new(),
            params: Vec::new(),
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
            control: QueryControl::new(),
            params: Vec::new(),
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

/// E-09 against a live server: options are applied, `bulkWrite` keeps partial
/// counts, `estimatedDocumentCount` runs, a find reports its total.
#[tokio::test]
#[ignore = "needs local MongoDB"]
async fn options_and_partial_bulk_results() {
    let uri = "mongodb://admin:123@127.0.0.1:27017/?authSource=admin";
    let Ok(client) = Client::with_uri_str(uri).await else {
        eprintln!("skip: no client");
        return;
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
    let db = "mystique_e09_live";
    let _ = client.database(db).drop().await;
    let session = MongoSession::new(client.clone(), 2, Some(db.into()));
    let run = |text: &str, max_rows: u32| ValidatedQuery {
        text: text.into(),
        mode: QueryMode::Write,
        max_rows,
        timeout_ms: 5_000,
        collection: None,
        database: Some(db.into()),
        control: QueryControl::new(),
        params: Vec::new(),
    };
    let message =
        |result: &mystique_connector_api::QueryResult| result.message.clone().unwrap_or_default();

    // upsert inserts; returnDocument "after" returns the new document.
    let upsert = session
        .execute_query(&run(
            "db.t.updateOne({_id: 1}, {$set: {n: 1}}, {upsert: true})",
            10,
        ))
        .await
        .expect("upsert");
    assert!(
        message(&upsert).contains("upsertedId"),
        "{}",
        message(&upsert)
    );
    let after = session
        .execute_query(&run(
            r#"db.t.findOneAndUpdate({_id: 1}, {$inc: {n: 1}}, {returnDocument: "after", projection: {n: 1}})"#,
            10,
        ))
        .await
        .expect("findOneAndUpdate");
    assert_eq!(
        after.rows.first().and_then(|r| r.get(1)).cloned(),
        Some(2.into())
    );

    // An ordered bulkWrite stops at the duplicate and says what ran first.
    let err = session
        .execute_query(&run(
            "db.t.bulkWrite([{insertOne: {document: {_id: 2}}}, {insertOne: {document: {_id: 1}}}, {insertOne: {document: {_id: 3}}}])",
            10,
        ))
        .await
        .expect_err("duplicate key");
    let text = err.to_string();
    assert!(text.contains("op 1:"), "{text}");
    assert!(text.contains("inserted=1"), "{text}");

    // Unordered runs the rest.
    let err = session
        .execute_query(&run(
            "db.t.insertMany([{_id: 1}, {_id: 4}, {_id: 5}], {ordered: false})",
            10,
        ))
        .await
        .expect_err("duplicate key");
    assert!(err.to_string().contains("inserted=2"), "{err}");

    let estimated = session
        .execute_query(&run("db.t.estimatedDocumentCount()", 10))
        .await
        .expect("estimatedDocumentCount");
    assert_eq!(
        estimated.rows.first().and_then(|r| r.first()).cloned(),
        Some(4.into())
    );

    // A truncated find reports the total; an explicit limit does not count.
    let page = session
        .execute_query(&run("db.t.find({})", 2))
        .await
        .expect("find");
    assert!(
        message(&page).contains("(4 matching)"),
        "{}",
        message(&page)
    );
    let limited = session
        .execute_query(&run("db.t.find({}).limit(2)", 2))
        .await
        .expect("find limit");
    assert!(
        !message(&limited).contains("matching"),
        "{}",
        message(&limited)
    );

    let _ = client.database(db).drop().await;
}

#[tokio::test]
#[ignore = "needs local MongoDB"]
async fn infer_fields_samples_a_collection_without_writing() {
    use mongodb::bson::Document;
    use mystique_connector_api::InferFieldsInput;

    let uri = "mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin";
    let client = Client::with_uri_str(uri).await.expect("client");
    let coll = client
        .database("mystique")
        .collection::<Document>("mq_gb_infer");
    let _ = coll.drop().await;
    coll.insert_many(vec![
        doc! { "name": "a", "address": { "city": "x" }, "tags": ["t", 1_i32] },
        doc! { "name": "b", "age": 3_i32 },
    ])
    .await
    .expect("seed");
    let session = MongoSession::new(client.clone(), 2, Some("mystique".into()));
    let out = session
        .infer_fields(&InferFieldsInput {
            collection: "mq_gb_infer".into(),
            database: Some("mystique".into()),
            sample: 50,
            timeout_ms: 5_000,
            control: QueryControl::new(),
        })
        .await
        .expect("infer");
    let paths: Vec<&str> = out.fields.iter().map(|f| f.path.as_str()).collect();
    for want in ["_id", "name", "address.city", "tags[]", "age"] {
        assert!(paths.contains(&want), "{want} missing from {paths:?}");
    }
    assert_eq!(out.sampled, 2);
    assert_eq!(coll.count_documents(doc! {}).await.expect("count"), 2);
    let _ = coll.drop().await;
}
