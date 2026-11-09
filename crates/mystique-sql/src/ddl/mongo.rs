//! MongoDB: index and validator commands, as JSON the plugin runs.

use super::changes::{column_changes, fk_changes};
use super::types::{DdlPlan, TableDefinition};
use super::{finish, DdlTarget};

pub(super) fn plan_mongo(draft: &TableDefinition, mut warnings: Vec<String>) -> DdlPlan {
    if draft.columns.iter().any(column_changes) {
        warnings.push("Mongo collections have no columns. Edit indexes or the validator.".into());
    }
    if draft.foreign_keys.iter().any(fk_changes) {
        warnings.push("Mongo collections have no foreign keys.".into());
    }
    let mut statements = Vec::new();
    if draft.drop_table {
        statements.push(mongo_command(&serde_json::json!({ "drop": draft.name })));
    }
    for index in &draft.indexes {
        if index.drop && index.original.is_some() {
            statements.push(mongo_command(&serde_json::json!({
                "dropIndexes": draft.name,
                "index": index.name,
            })));
            continue;
        }
        if index.original.is_none() && !index.drop {
            let mut key = serde_json::Map::new();
            for column in &index.columns {
                key.insert(column.clone(), serde_json::json!(1));
            }
            statements.push(mongo_command(&serde_json::json!({
                "createIndexes": draft.name,
                "indexes": [{
                    "key": key,
                    "name": index.name,
                    "unique": index.unique,
                }],
            })));
        }
    }
    if draft.validator != draft.original_validator {
        match draft
            .validator
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            None => warnings.push("Clearing a Mongo validator is not supported here.".into()),
            Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
                Ok(validator) => statements.push(mongo_command(&serde_json::json!({
                    "collMod": draft.name,
                    "validator": validator,
                }))),
                Err(_) => warnings.push("Mongo validator must be a JSON object.".into()),
            },
        }
    }
    finish(DdlTarget::Mongo, statements, warnings, false, false, None)
}

pub(super) fn mongo_command(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".into())
}
