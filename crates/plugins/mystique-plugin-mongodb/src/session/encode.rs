//! BSON / JSON encoding and Mongo write-command helpers.

use mongodb::bson::Bson;
use mystique_connector_api::{ConnectorResult, ValidatedQuery};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use super::MongoSession;

pub(super) fn bson_to_json(b: Bson) -> JsonValue {
    match b {
        Bson::Double(f) => json!(f),
        Bson::String(s) => JsonValue::String(s),
        Bson::Array(arr) => JsonValue::Array(arr.into_iter().map(bson_to_json).collect()),
        Bson::Document(d) => {
            let mut map = serde_json::Map::new();
            for (k, v) in d {
                map.insert(k, bson_to_json(v));
            }
            JsonValue::Object(map)
        }
        Bson::Boolean(b) => JsonValue::Bool(b),
        Bson::Null => JsonValue::Null,
        Bson::Int32(i) => json!(i),
        Bson::Int64(i) => json!(i),
        Bson::ObjectId(oid) => JsonValue::String(oid.to_hex()),
        Bson::DateTime(dt) => JsonValue::String(dt.to_string()),
        other => JsonValue::String(other.to_string()),
    }
}

pub(super) fn mongo_id_json(id: &JsonValue) -> JsonValue {
    if let Some(s) = id.as_str() {
        if s.len() == 24 && s.chars().all(|c| c.is_ascii_hexdigit()) {
            return json!({ "$oid": s });
        }
    }
    id.clone()
}

pub(super) fn coerce_mongo_json(v: &JsonValue) -> JsonValue {
    if let Some(s) = v.as_str() {
        let t = s.trim();
        if (t.starts_with('{') && t.ends_with('}')) || (t.starts_with('[') && t.ends_with(']')) {
            if let Ok(parsed) = serde_json::from_str::<JsonValue>(t) {
                return parsed;
            }
        }
    }
    v.clone()
}

pub(super) fn bson_type_label(sample: Option<&Bson>) -> String {
    match sample {
        None | Some(Bson::Null) => "mixed".into(),
        Some(Bson::Double(_)) => "double".into(),
        Some(Bson::String(_)) => "string".into(),
        Some(Bson::Array(_)) => "array".into(),
        Some(Bson::Document(_)) => "object".into(),
        Some(Bson::Boolean(_)) => "bool".into(),
        Some(Bson::Int32(_)) => "int32".into(),
        Some(Bson::Int64(_)) => "int64".into(),
        Some(Bson::ObjectId(_)) => "ObjectId".into(),
        Some(Bson::DateTime(_)) => "date".into(),
        Some(Bson::Binary(_)) => "binData".into(),
        Some(Bson::RegularExpression(_)) => "regex".into(),
        Some(Bson::JavaScriptCode(_)) | Some(Bson::JavaScriptCodeWithScope(_)) => {
            "javascript".into()
        }
        Some(Bson::Timestamp(_)) => "timestamp".into(),
        Some(Bson::Decimal128(_)) => "decimal".into(),
        Some(Bson::Symbol(_)) => "symbol".into(),
        Some(Bson::Undefined) => "undefined".into(),
        Some(Bson::MaxKey) => "maxKey".into(),
        Some(Bson::MinKey) => "minKey".into(),
        Some(Bson::DbPointer(_)) => "dbPointer".into(),
    }
}

pub(super) fn normalize_meta_command(text: &str) -> String {
    text.trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
}

pub(super) fn resolve_db_name(
    session: &MongoSession,
    query: &ValidatedQuery,
) -> ConnectorResult<String> {
    session.resolve_database(query.database.as_deref())
}

#[derive(Debug, Deserialize)]
pub(super) struct MongoWriteCmd {
    pub(super) op: String,
    #[serde(default)]
    pub(super) filter: Option<JsonValue>,
    #[serde(default)]
    pub(super) update: Option<JsonValue>,
    #[serde(default)]
    pub(super) doc: Option<JsonValue>,
}

pub(super) fn is_mongo_write_command(text: &str) -> bool {
    mystique_connector_api::mongo_is_write_command(text)
}
