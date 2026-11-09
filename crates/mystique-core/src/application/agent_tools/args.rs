use mystique_connector_api::CatalogObject;
use serde_json::Value as JsonValue;

use crate::domain::ConnectionProfile;
use crate::error::{AppError, AppResult};

use super::types::AgentObjectInfo;

pub(super) fn object_info(obj: CatalogObject) -> AgentObjectInfo {
    let schema = obj.schema.clone().unwrap_or_default();
    let qualified_name = super::super::dbml::table_ident(&schema, &obj.name);
    AgentObjectInfo {
        name: obj.name,
        qualified_name,
        schema: obj.schema,
        database: obj.database,
        kind: obj.kind,
        row_count: obj.row_count,
    }
}

pub(crate) fn profile_database(profile: &ConnectionProfile) -> Option<String> {
    let cfg = &profile.plugin_config;
    if let Some(s) = cfg
        .get("database")
        .or_else(|| cfg.get("db"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(s.to_string());
    }
    if let Some(path) = cfg
        .get("path")
        .or_else(|| cfg.get("file"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(path.to_string());
    }
    if let Some(uri) = cfg.get("uri").and_then(|v| v.as_str()) {
        if let Some(db) = database_from_mongo_uri(uri) {
            return Some(db);
        }
    }
    None
}

fn database_from_mongo_uri(uri: &str) -> Option<String> {
    let after = uri.split("://").nth(1)?;
    let path = after.split_once('/')?.1;
    let name = path.split('?').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

pub(super) fn req_str(args: &JsonValue, key: &str) -> AppResult<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Invalid(format!("`{key}` is required")))
}

pub(super) fn opt_str(args: &JsonValue, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}
