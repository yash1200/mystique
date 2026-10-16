//! Read collection indexes and validators, then run planner commands.

use mongodb::bson::{doc, Document};
use mongodb::results::CollectionType;
use mystique_connector_api::{
    ApplyDdlResult, ConnectorError, ConnectorResult, DdlPlan, IndexDraft, TableDefinition,
};

use super::query_failed;
use super::MongoSession;

pub(super) async fn describe_table_definition(
    session: &MongoSession,
    database: Option<&str>,
    _schema: Option<&str>,
    table: &str,
) -> ConnectorResult<TableDefinition> {
    let mut tables = load(session, database, Some(table)).await?;
    match tables.pop() {
        Some(definition) => Ok(definition),
        None => Err(ConnectorError::Query(format!(
            "Collection `{table}` was not found."
        ))),
    }
}

pub(super) async fn list_table_definitions(
    session: &MongoSession,
    database: Option<&str>,
    _schema: Option<&str>,
) -> ConnectorResult<Vec<TableDefinition>> {
    load(session, database, None).await
}

pub(super) async fn apply_ddl(
    session: &MongoSession,
    plan: &DdlPlan,
    draft: &TableDefinition,
) -> ConnectorResult<ApplyDdlResult> {
    session.ensure_open()?;
    if plan.statements.is_empty() {
        return Ok(ApplyDdlResult {
            applied: Vec::new(),
            message: "Nothing to change.".into(),
        });
    }
    let db_name = session.resolve_database(draft.database.as_deref())?;
    let db = session.client.database(&db_name);
    let mut applied = Vec::new();
    for sql in &plan.statements {
        let value: serde_json::Value = serde_json::from_str(sql).map_err(|err| {
            ConnectorError::Query(format!("Could not read a Mongo command: {err}"))
        })?;
        let command = mongodb::bson::to_document(&value).map_err(|err| {
            ConnectorError::Query(format!("Could not read a Mongo command: {err}"))
        })?;
        if let Err(err) = db.run_command(command).await {
            let tail = if applied.is_empty() {
                "Nothing was applied."
            } else {
                "Earlier commands were not rolled back."
            };
            return Err(ConnectorError::Query(format!(
                "Applied {} statement(s), then failed. {tail} {}",
                applied.len(),
                query_failed(err)
            )));
        }
        applied.push(sql.clone());
    }
    let count = applied.len();
    Ok(ApplyDdlResult {
        applied,
        message: format!("Applied {count} statement(s)."),
    })
}

async fn load(
    session: &MongoSession,
    database: Option<&str>,
    only: Option<&str>,
) -> ConnectorResult<Vec<TableDefinition>> {
    session.ensure_open()?;
    let db_name = session.resolve_database(database)?;
    let db = session.client.database(&db_name);
    let mut listed = match only {
        Some(name) => db
            .list_collections()
            .filter(doc! { "name": name })
            .await
            .map_err(query_failed)?,
        None => db.list_collections().await.map_err(query_failed)?,
    };
    let mut out = Vec::new();
    while listed.advance().await.map_err(query_failed)? {
        let spec = listed.deserialize_current().map_err(query_failed)?;
        if spec.name.starts_with("system.") || spec.collection_type == CollectionType::View {
            continue;
        }
        let mut definition =
            TableDefinition::loaded(Some(db_name.clone()), None, spec.name.clone(), "collection");
        if let Some(validator) = spec.options.validator.as_ref() {
            if let Ok(text) = serde_json::to_string_pretty(validator) {
                definition.validator = Some(text.clone());
                definition.original_validator = Some(text);
            }
        }
        definition.indexes = indexes(session, &db_name, &spec.name).await?;
        out.push(definition);
    }
    Ok(out)
}

async fn indexes(
    session: &MongoSession,
    database: &str,
    table: &str,
) -> ConnectorResult<Vec<IndexDraft>> {
    let coll = session
        .client
        .database(database)
        .collection::<Document>(table);
    let mut cursor = coll.list_indexes().await.map_err(query_failed)?;
    let mut indexes = Vec::new();
    while cursor.advance().await.map_err(query_failed)? {
        let model = cursor.deserialize_current().map_err(query_failed)?;
        let Some(name) = model
            .options
            .as_ref()
            .and_then(|options| options.name.clone())
        else {
            continue;
        };
        let columns: Vec<String> = model.keys.keys().cloned().collect();
        if columns.is_empty() {
            continue;
        }
        let unique = model
            .options
            .as_ref()
            .and_then(|options| options.unique)
            .unwrap_or(false);
        indexes.push(IndexDraft::loaded(
            name.clone(),
            columns,
            unique,
            name == "_id_",
        ));
    }
    Ok(indexes)
}
