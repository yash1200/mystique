//! Mongo find / meta commands / JSON write ops.

use std::time::Instant;

use mongodb::bson::{Bson, Document};
use mystique_connector_api::{
    ConnectorError, ConnectorResult, QueryColumn, QueryMode, QueryResult, ValidatedQuery,
};
use serde_json::{json, Value as JsonValue};

use super::encode::{
    bson_to_json, bson_type_label, is_mongo_write_command, normalize_meta_command, resolve_db_name,
    MongoWriteCmd,
};
use super::query_failed;
use super::MongoSession;
use crate::shell_filter::json_to_document;
use crate::shell_query::parse_read_query;

pub(super) async fn execute_query(
    session: &MongoSession,
    query: &ValidatedQuery,
) -> ConnectorResult<QueryResult> {
    session.ensure_open()?;
    if is_mongo_write_command(&query.text) {
        if query.mode == QueryMode::Read {
            return Err(ConnectorError::Policy(
                "Write operation blocked. This connection is read-only.".into(),
            ));
        }
        return execute_write(session, query).await;
    }

    let start = Instant::now();
    let text_key = normalize_meta_command(&query.text);

    if matches!(
        text_key.as_str(),
        "listdatabases" | "showdatabases" | "showdbs"
    ) {
        return list_databases(session, query.mode, start).await;
    }
    if matches!(text_key.as_str(), "listcollections" | "showcollections") {
        let db_name = resolve_db_name(session, query)?;
        return list_collections(session, &db_name, query.mode, query.max_rows, start).await;
    }

    let db_name = resolve_db_name(session, query)?;
    let parsed = parse_read_query(&query.text)?;
    let coll_name = parsed
        .collection
        .clone()
        .or_else(|| {
            query
                .collection
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(
                "Mongo queries need a collection (use the Collection field or db.collection.find(...))."
                    .into(),
            )
        })?;

    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&coll_name);
    let limit = parsed
        .limit
        .unwrap_or(query.max_rows as i64)
        .clamp(1, query.max_rows as i64);
    let mut find = coll.find(parsed.filter);
    if let Some(proj) = parsed.projection {
        find = find.projection(proj);
    }
    if let Some(sort) = parsed.sort {
        find = find.sort(sort);
    }
    if let Some(skip) = parsed.skip {
        find = find.skip(skip);
    }
    let mut cursor = find.limit(limit).await.map_err(query_failed)?;

    let mut docs = Vec::new();
    while cursor.advance().await.map_err(query_failed)? {
        let d = cursor.deserialize_current().map_err(query_failed)?;
        docs.push(d);
        if docs.len() >= query.max_rows as usize {
            break;
        }
    }

    // Flatten documents into a table: union of keys
    let mut key_set: Vec<String> = Vec::new();
    for d in &docs {
        for k in d.keys() {
            if !key_set.iter().any(|x| x == k) {
                key_set.push(k.clone());
            }
        }
    }
    // Prefer _id first
    if let Some(i) = key_set.iter().position(|k| k == "_id") {
        let id = key_set.remove(i);
        key_set.insert(0, id);
    }

    let columns: Vec<QueryColumn> = key_set
        .iter()
        .map(|k| {
            let sample = docs.iter().find_map(|d| d.get(k)).cloned();
            QueryColumn {
                name: k.clone(),
                data_type: Some(bson_type_label(sample.as_ref())),
            }
        })
        .collect();

    let mut rows = Vec::new();
    for d in docs {
        let mut cells = Vec::with_capacity(key_set.len());
        for k in &key_set {
            let v = d.get(k).cloned().unwrap_or(Bson::Null);
            cells.push(bson_to_json(v));
        }
        rows.push(cells);
    }

    let truncated = rows.len() as u32 >= query.max_rows;
    Ok(QueryResult {
        engine: "mongodb".into(),
        mode: query.mode,
        columns,
        row_count: rows.len() as u64,
        rows,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
        message: Some(format!("find on {db_name}.{coll_name}")),
    })
}

pub(super) async fn execute_write(
    session: &MongoSession,
    query: &ValidatedQuery,
) -> ConnectorResult<QueryResult> {
    let start = Instant::now();
    let cmd: MongoWriteCmd = serde_json::from_str(query.text.trim()).map_err(|e| {
        ConnectorError::Query(format!(
            "Mongo write expects JSON {{ op, filter?, update?, doc? }}: {e}"
        ))
    })?;
    let op = cmd.op.to_ascii_lowercase();
    let db_name = resolve_db_name(session, query)?;
    let coll_name = query
        .collection
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ConnectorError::InvalidConfig("Mongo write requires collection".into()))?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(coll_name);

    let message = match op.as_str() {
        "updateone" => {
            let filter_v = cmd
                .filter
                .ok_or_else(|| ConnectorError::Query("updateOne requires `filter`".into()))?;
            let update_v = cmd
                .update
                .ok_or_else(|| ConnectorError::Query("updateOne requires `update`".into()))?;
            let filter = json_to_document(&filter_v)?;
            let update = json_to_document(&update_v)?;
            let result = coll
                .update_one(filter, update)
                .await
                .map_err(query_failed)?;
            format!(
                "updateOne on {db_name}.{coll_name} matched={} modified={}",
                result.matched_count, result.modified_count
            )
        }
        "insertone" => {
            let doc_v = cmd
                .doc
                .ok_or_else(|| ConnectorError::Query("insertOne requires `doc`".into()))?;
            let doc = json_to_document(&doc_v)?;
            let result = coll.insert_one(doc).await.map_err(query_failed)?;
            format!(
                "insertOne on {db_name}.{coll_name} id={:?}",
                result.inserted_id
            )
        }
        "deleteone" => {
            let filter_v = cmd
                .filter
                .ok_or_else(|| ConnectorError::Query("deleteOne requires `filter`".into()))?;
            let filter = json_to_document(&filter_v)?;
            let result = coll.delete_one(filter).await.map_err(query_failed)?;
            format!(
                "deleteOne on {db_name}.{coll_name} deleted={}",
                result.deleted_count
            )
        }
        "deletemany" => {
            let filter_v = cmd
                .filter
                .ok_or_else(|| ConnectorError::Query("deleteMany requires `filter`".into()))?;
            let filter = json_to_document(&filter_v)?;
            let result = coll.delete_many(filter).await.map_err(query_failed)?;
            format!(
                "deleteMany on {db_name}.{coll_name} deleted={}",
                result.deleted_count
            )
        }
        other => {
            return Err(ConnectorError::NotSupported(format!(
                "Mongo write op `{other}` is not implemented."
            )));
        }
    };

    Ok(QueryResult {
        engine: "mongodb".into(),
        mode: query.mode,
        columns: Vec::new(),
        row_count: 0,
        rows: Vec::new(),
        truncated: false,
        duration_ms: start.elapsed().as_millis() as u64,
        message: Some(message),
    })
}

pub(super) async fn list_databases(
    session: &MongoSession,
    mode: QueryMode,
    start: Instant,
) -> ConnectorResult<QueryResult> {
    let names = session
        .client
        .list_database_names()
        .await
        .map_err(query_failed)?;

    let columns = vec![QueryColumn {
        name: "name".into(),
        data_type: Some("string".into()),
    }];
    let rows: Vec<Vec<JsonValue>> = names
        .into_iter()
        .map(|n| vec![JsonValue::String(n)])
        .collect();
    let count = rows.len() as u64;

    Ok(QueryResult {
        engine: "mongodb".into(),
        mode,
        columns,
        row_count: count,
        rows,
        truncated: false,
        duration_ms: start.elapsed().as_millis() as u64,
        message: Some("listDatabases".into()),
    })
}

pub(super) async fn list_collections(
    session: &MongoSession,
    db_name: &str,
    mode: QueryMode,
    max_rows: u32,
    start: Instant,
) -> ConnectorResult<QueryResult> {
    let mut names = session
        .client
        .database(db_name)
        .list_collection_names()
        .await
        .map_err(query_failed)?;
    names.sort();
    let truncated = names.len() as u32 > max_rows;
    if truncated {
        names.truncate(max_rows as usize);
    }

    // Estimated counts (best-effort; empty on error)
    let mut rows = Vec::with_capacity(names.len());
    for name in &names {
        let count = session
            .client
            .database(db_name)
            .collection::<Document>(name)
            .estimated_document_count()
            .await
            .ok();
        rows.push(vec![
            JsonValue::String(name.clone()),
            count.map(|c| json!(c)).unwrap_or(JsonValue::Null),
        ]);
    }

    Ok(QueryResult {
        engine: "mongodb".into(),
        mode,
        columns: vec![
            QueryColumn {
                name: "name".into(),
                data_type: Some("string".into()),
            },
            QueryColumn {
                name: "estimatedCount".into(),
                data_type: Some("number".into()),
            },
        ],
        row_count: rows.len() as u64,
        rows,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
        message: Some(format!("listCollections on {db_name}")),
    })
}
