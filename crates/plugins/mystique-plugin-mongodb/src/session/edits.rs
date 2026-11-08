//! Document insert / update / delete from the grid.
//!
//! A replica set wraps the save in one transaction. A standalone server
//! cannot, and a later failure says which earlier documents stayed.

use std::sync::atomic::Ordering;

use mongodb::bson::Document;
use mongodb::ClientSession;
use mystique_connector_api::{
    affected_mismatch, ApplyTableEditsInput, ApplyTableEditsResult, ConnectorError, ConnectorResult,
};
use serde_json::{json, Value as JsonValue};

use super::encode::{coerce_mongo_json, mongo_id_json};
use super::query_failed;
use super::MongoSession;
use crate::shell_filter::json_to_document;

pub(super) async fn apply_table_edits(
    session: &MongoSession,
    edits: &ApplyTableEditsInput,
) -> ConnectorResult<ApplyTableEditsResult> {
    session.ensure_open()?;
    let db_name = session.resolve_database(edits.database.as_deref())?;
    let statements = preview_statements(edits);
    if edits.dry_run {
        return Ok(ApplyTableEditsResult {
            updated: 0,
            inserted: 0,
            deleted: 0,
            statements,
            save_id: None,
            undo: None,
        });
    }
    let transactional = session.transactions.load(Ordering::SeqCst);
    if transactional {
        let mut client_session = session.client.start_session().await.map_err(query_failed)?;
        client_session
            .start_transaction()
            .await
            .map_err(query_failed)?;
        match apply_ops(session, &db_name, edits, Some(&mut client_session)).await {
            Ok(counts) => {
                client_session
                    .commit_transaction()
                    .await
                    .map_err(query_failed)?;
                Ok(result(counts, statements))
            }
            Err((_, err)) => {
                let _ = client_session.abort_transaction().await;
                Err(err)
            }
        }
    } else {
        match apply_ops(session, &db_name, edits, None).await {
            Ok(counts) => Ok(result(counts, statements)),
            Err((applied, err)) => Err(standalone_failure(applied, err)),
        }
    }
}

fn reborrow<'a>(slot: &'a mut Option<&mut ClientSession>) -> Option<&'a mut ClientSession> {
    slot.as_mut().map(|session| &mut **session)
}

struct Counts {
    updated: u32,
    inserted: u32,
    deleted: u32,
}

fn result(counts: Counts, statements: Vec<String>) -> ApplyTableEditsResult {
    ApplyTableEditsResult {
        updated: counts.updated,
        inserted: counts.inserted,
        deleted: counts.deleted,
        statements,
        save_id: None,
        undo: None,
    }
}

fn standalone_failure(applied: u32, err: ConnectorError) -> ConnectorError {
    if applied == 0 {
        return err;
    }
    ConnectorError::Query(format!(
        "Saved {applied} document(s), then failed. This MongoDB server has no multi-document transaction, so those writes stayed. {err}"
    ))
}

async fn apply_ops(
    session: &MongoSession,
    db_name: &str,
    edits: &ApplyTableEditsInput,
    mut client_session: Option<&mut ClientSession>,
) -> Result<Counts, (u32, ConnectorError)> {
    let coll = session
        .client
        .database(db_name)
        .collection::<Document>(&edits.table);
    let mut updated = 0u32;
    let mut inserted = 0u32;
    let mut deleted = 0u32;

    for update in &edits.updates {
        let id = match document_id(&update.original) {
            Ok(id) => id,
            Err(err) => return Err((updated + inserted + deleted, err)),
        };
        let mut set_obj = serde_json::Map::new();
        for (key, value) in &update.values {
            if key == "_id" {
                continue;
            }
            set_obj.insert(key.clone(), coerce_mongo_json(value));
        }
        if set_obj.is_empty() {
            continue;
        }
        let filter = match json_to_document(&json!({ "_id": mongo_id_json(&id) })) {
            Ok(filter) => filter,
            Err(err) => return Err((updated + inserted + deleted, err)),
        };
        let update_doc = match json_to_document(&json!({ "$set": JsonValue::Object(set_obj) })) {
            Ok(update_doc) => update_doc,
            Err(err) => return Err((updated + inserted + deleted, err)),
        };
        let action = coll.update_one(filter, update_doc);
        let written = match reborrow(&mut client_session) {
            Some(client_session) => action.session(client_session).await,
            None => action.await,
        };
        let written = match written {
            Ok(written) => written,
            Err(err) => return Err((updated + inserted + deleted, query_failed(err))),
        };
        if written.matched_count != 1 {
            return Err((
                updated + inserted + deleted,
                affected_mismatch("update", written.matched_count),
            ));
        }
        updated += 1;
    }

    for row in &edits.inserts {
        let mut doc_map = serde_json::Map::new();
        for (key, value) in &row.values {
            if matches!(value, JsonValue::Null) || value.as_str() == Some("") {
                continue;
            }
            if key == "_id" {
                doc_map.insert(key.clone(), mongo_id_json(value));
            } else {
                doc_map.insert(key.clone(), coerce_mongo_json(value));
            }
        }
        if doc_map.is_empty() {
            return Err((
                updated + inserted + deleted,
                ConnectorError::Query(
                    "New document is empty — fill at least one field before saving.".into(),
                ),
            ));
        }
        let doc = match json_to_document(&JsonValue::Object(doc_map)) {
            Ok(doc) => doc,
            Err(err) => return Err((updated + inserted + deleted, err)),
        };
        let action = coll.insert_one(doc);
        let written = match reborrow(&mut client_session) {
            Some(client_session) => action.session(client_session).await,
            None => action.await,
        };
        if let Err(err) = written {
            return Err((updated + inserted + deleted, query_failed(err)));
        }
        inserted += 1;
    }

    for delete in &edits.deletes {
        let id = match document_id(&delete.original) {
            Ok(id) => id,
            Err(err) => return Err((updated + inserted + deleted, err)),
        };
        let filter = match json_to_document(&json!({ "_id": mongo_id_json(&id) })) {
            Ok(filter) => filter,
            Err(err) => return Err((updated + inserted + deleted, err)),
        };
        let action = coll.delete_one(filter);
        let written = match reborrow(&mut client_session) {
            Some(client_session) => action.session(client_session).await,
            None => action.await,
        };
        let written = match written {
            Ok(written) => written,
            Err(err) => return Err((updated + inserted + deleted, query_failed(err))),
        };
        if written.deleted_count != 1 {
            return Err((
                updated + inserted + deleted,
                affected_mismatch("delete", written.deleted_count),
            ));
        }
        deleted += 1;
    }

    Ok(Counts {
        updated,
        inserted,
        deleted,
    })
}

fn document_id(
    original: &std::collections::BTreeMap<String, JsonValue>,
) -> ConnectorResult<JsonValue> {
    let id = original
        .get("_id")
        .or_else(|| original.get("id"))
        .ok_or_else(|| ConnectorError::Query("Cannot change document: missing _id.".into()))?;
    if matches!(id, JsonValue::Null) || id.as_str() == Some("") {
        return Err(ConnectorError::Query(
            "Cannot change document: missing _id.".into(),
        ));
    }
    Ok(id.clone())
}

fn preview_statements(edits: &ApplyTableEditsInput) -> Vec<String> {
    let collection = serde_json::to_string(&edits.table).unwrap_or_else(|_| "\"\"".to_string());
    let mut statements = Vec::new();
    for update in &edits.updates {
        let id = update
            .original
            .get("_id")
            .or_else(|| update.original.get("id"))
            .cloned()
            .unwrap_or(JsonValue::Null);
        let mut set_obj = serde_json::Map::new();
        for (key, value) in &update.values {
            if key != "_id" {
                set_obj.insert(key.clone(), value.clone());
            }
        }
        statements.push(format!(
            "db.getCollection({collection}).updateOne({}, {})",
            json!({ "_id": mongo_id_json(&id) }),
            json!({ "$set": JsonValue::Object(set_obj) })
        ));
    }
    for row in &edits.inserts {
        statements.push(format!(
            "db.getCollection({collection}).insertOne({})",
            JsonValue::Object(row.values.clone().into_iter().collect())
        ));
    }
    for delete in &edits.deletes {
        let id = delete
            .original
            .get("_id")
            .or_else(|| delete.original.get("id"))
            .cloned()
            .unwrap_or(JsonValue::Null);
        statements.push(format!(
            "db.getCollection({collection}).deleteOne({})",
            json!({ "_id": mongo_id_json(&id) })
        ));
    }
    statements
}
