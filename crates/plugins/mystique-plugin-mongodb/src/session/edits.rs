//! Document insert / update / delete from the grid.

use mongodb::bson::Document;
use mystique_connector_api::{
    ApplyTableEditsInput, ApplyTableEditsResult, ConnectorError, ConnectorResult,
};
use serde_json::{json, Value as JsonValue};

use super::encode::{coerce_mongo_json, mongo_id_json};
use super::MongoSession;
use crate::shell_filter::json_to_document;

pub(super) async fn apply_table_edits(
    session: &MongoSession,
    edits: &ApplyTableEditsInput,
) -> ConnectorResult<ApplyTableEditsResult> {
    session.ensure_open()?;
    let db_name = session.resolve_database(edits.database.as_deref())?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&edits.table);

    let mut updated = 0u32;
    let mut inserted = 0u32;
    let mut deleted = 0u32;

    for u in &edits.updates {
        let id = u
            .original
            .get("_id")
            .or_else(|| u.original.get("id"))
            .ok_or_else(|| ConnectorError::Query("Cannot update document: missing _id.".into()))?;
        if matches!(id, JsonValue::Null) || id.as_str() == Some("") {
            return Err(ConnectorError::Query(
                "Cannot update document: missing _id.".into(),
            ));
        }
        let mut set_obj = serde_json::Map::new();
        for (k, v) in &u.values {
            if k == "_id" {
                continue;
            }
            set_obj.insert(k.clone(), coerce_mongo_json(v));
        }
        if set_obj.is_empty() {
            continue;
        }
        let filter = json_to_document(&json!({ "_id": mongo_id_json(id) }))?;
        let update = json_to_document(&json!({ "$set": JsonValue::Object(set_obj) }))?;
        coll.update_one(filter, update)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        updated += 1;
    }

    for row in &edits.inserts {
        let mut doc_map = serde_json::Map::new();
        for (k, v) in &row.values {
            if matches!(v, JsonValue::Null) || v.as_str() == Some("") {
                continue;
            }
            if k == "_id" {
                doc_map.insert(k.clone(), mongo_id_json(v));
            } else {
                doc_map.insert(k.clone(), coerce_mongo_json(v));
            }
        }
        if doc_map.is_empty() {
            return Err(ConnectorError::Query(
                "New document is empty — fill at least one field before saving.".into(),
            ));
        }
        let doc = json_to_document(&JsonValue::Object(doc_map))?;
        coll.insert_one(doc)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        inserted += 1;
    }

    for d in &edits.deletes {
        let id = d
            .original
            .get("_id")
            .or_else(|| d.original.get("id"))
            .ok_or_else(|| ConnectorError::Query("Cannot delete document: missing _id.".into()))?;
        if matches!(id, JsonValue::Null) || id.as_str() == Some("") {
            return Err(ConnectorError::Query(
                "Cannot delete document: missing _id.".into(),
            ));
        }
        let filter = json_to_document(&json!({ "_id": mongo_id_json(id) }))?;
        coll.delete_one(filter)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        deleted += 1;
    }

    Ok(ApplyTableEditsResult {
        updated,
        inserted,
        deleted,
    })
}
