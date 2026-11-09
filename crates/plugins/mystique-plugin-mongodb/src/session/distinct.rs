//! Distinct values for the grid filter. At most 1000 non-null values.

use std::time::Duration;

use mongodb::bson::doc;
use mongodb::bson::Document;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, DistinctColumnValuesInput, FETCH_TIMEOUT_MS,
};
use serde_json::Value as JsonValue;

use super::catalog;
use super::encode::bson_to_json;
use super::query_failed;
use super::MongoSession;

pub(super) async fn distinct_column_values(
    session: &MongoSession,
    input: &DistinctColumnValuesInput,
) -> ConnectorResult<Vec<JsonValue>> {
    session.ensure_open()?;
    let described = catalog::describe_table(
        session,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    )
    .await?;
    if !described.iter().any(|column| column.name == input.column) {
        return Err(ConnectorError::Query(format!(
            "Unknown column “{}”.",
            input.column
        )));
    }
    let db_name = session.resolve_database(input.database.as_deref())?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&input.table);
    let limit = usize::try_from(input.limit.clamp(1, 1_000)).unwrap_or(1_000);
    let values = coll
        .distinct(&input.column, doc! {})
        .max_time(Duration::from_millis(FETCH_TIMEOUT_MS))
        .await
        .map_err(query_failed)?;
    let mut out = Vec::new();
    for value in values {
        if out.len() >= limit {
            break;
        }
        let json = bson_to_json(value);
        if !json.is_null() {
            out.push(json);
        }
    }
    Ok(out)
}
