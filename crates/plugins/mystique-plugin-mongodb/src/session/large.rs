//! Re-read one clipped cell by primary key (`_id` or an explicit key list).

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{doc, Bson, Document};
use mystique_connector_api::{
    page_cell, ConnectorError, ConnectorResult, FetchLargeValueInput, FetchLargeValueResult,
};
use serde_json::Value as JsonValue;

use super::encode::bson_to_json;
use super::query_failed;
use super::MongoSession;

pub(super) async fn fetch_large_value(
    session: &MongoSession,
    input: &FetchLargeValueInput,
) -> ConnectorResult<FetchLargeValueResult> {
    session.ensure_open()?;
    let locator = &input.locator;
    if locator.table.trim().is_empty() || locator.column.trim().is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "A large value needs a collection and a field.".into(),
        ));
    }
    if locator.keys.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "This value cannot be fetched without a primary key.".into(),
        ));
    }
    let db_name = session.resolve_database(locator.database.as_deref())?;
    let mut filter = Document::new();
    for key in &locator.keys {
        if key.column.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "A primary key field name is required.".into(),
            ));
        }
        filter.insert(
            key.column.clone(),
            json_key_to_bson(&key.column, &key.value),
        );
    }
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&locator.table);
    let found = coll
        .find_one(filter)
        .projection(doc! { locator.column.clone(): 1 })
        .await
        .map_err(query_failed)?;
    let Some(document) = found else {
        return Err(ConnectorError::Query("No row matched that key.".into()));
    };
    let value = document.get(&locator.column).cloned().unwrap_or(Bson::Null);
    Ok(page_cell(&bson_to_json(value), input.offset, input.limit))
}

fn json_key_to_bson(column: &str, value: &JsonValue) -> Bson {
    match value {
        JsonValue::Null => Bson::Null,
        JsonValue::Bool(flag) => Bson::Boolean(*flag),
        JsonValue::Number(number) => number
            .as_i64()
            .map(Bson::Int64)
            .or_else(|| number.as_u64().map(|n| Bson::Int64(n as i64)))
            .or_else(|| number.as_f64().map(Bson::Double))
            .unwrap_or(Bson::Null),
        JsonValue::String(text) => {
            if column == "_id" && text.len() == 24 && text.chars().all(|c| c.is_ascii_hexdigit()) {
                if let Ok(oid) = ObjectId::parse_str(text) {
                    return Bson::ObjectId(oid);
                }
            }
            Bson::String(text.clone())
        }
        other => bson_to_extended(other),
    }
}

fn bson_to_extended(value: &JsonValue) -> Bson {
    serde_json::from_value(value.clone())
        .ok()
        .unwrap_or(Bson::Null)
}
