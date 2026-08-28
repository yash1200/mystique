//! Export and import via the shared connector-api helpers.

use mongodb::bson::Document;
use mystique_connector_api::{
    export_object_with_preview, import_object_from_file, preview_import_with_describe,
    ConnectorResult, ExportObjectInput, ExportObjectResult, ImportChunkInput, ImportChunkResult,
    ImportObjectInput, ImportObjectResult, ImportOnDuplicate, PreviewImportInput,
    PreviewImportResult,
};
use serde_json::{json, Value as JsonValue};

use super::encode::{coerce_mongo_json, mongo_id_json};
use super::query_failed;
use super::MongoSession;
use crate::shell_filter::json_to_document;

pub(super) async fn export_object(
    session: &MongoSession,
    input: &ExportObjectInput,
) -> ConnectorResult<ExportObjectResult> {
    session.ensure_open()?;
    export_object_with_preview(session, input, None).await
}

pub(super) async fn preview_import(
    session: &MongoSession,
    input: &PreviewImportInput,
) -> ConnectorResult<PreviewImportResult> {
    session.ensure_open()?;
    preview_import_with_describe(session, input).await
}

pub(super) async fn import_object(
    session: &MongoSession,
    input: &ImportObjectInput,
) -> ConnectorResult<ImportObjectResult> {
    session.ensure_open()?;
    import_object_from_file(session, input).await
}

pub(super) async fn import_chunk(
    session: &MongoSession,
    input: &ImportChunkInput,
) -> ConnectorResult<ImportChunkResult> {
    session.ensure_open()?;
    let db_name = session.resolve_database(input.database.as_deref())?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&input.name);
    let mut inserted = 0u64;
    let mut updated = 0u64;
    for row in &input.rows {
        let mut doc_map = serde_json::Map::new();
        for (k, v) in row {
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
            continue;
        }
        match input.on_duplicate {
            ImportOnDuplicate::StopOnError => {
                let doc = json_to_document(&JsonValue::Object(doc_map))?;
                coll.insert_one(doc).await.map_err(query_failed)?;
                inserted += 1;
            }
            ImportOnDuplicate::UpdateOnDuplicate => {
                let id = row
                    .get("_id")
                    .or_else(|| row.get("id"))
                    .cloned()
                    .unwrap_or(JsonValue::Null);
                if matches!(id, JsonValue::Null) || id.as_str() == Some("") {
                    let doc = json_to_document(&JsonValue::Object(doc_map))?;
                    coll.insert_one(doc).await.map_err(query_failed)?;
                    inserted += 1;
                } else {
                    let filter = json_to_document(&json!({ "_id": mongo_id_json(&id) }))?;
                    let replacement = json_to_document(&JsonValue::Object(doc_map))?;
                    coll.replace_one(filter, replacement)
                        .upsert(true)
                        .await
                        .map_err(query_failed)?;
                    updated += 1;
                }
            }
        }
    }
    Ok(ImportChunkResult { inserted, updated })
}
