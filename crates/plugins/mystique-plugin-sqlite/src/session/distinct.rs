//! Distinct values for the grid filter. One catalog column, at most 1000.

use mystique_connector_api::{
    distinct_values_sql, DistinctColumnValuesInput, QueryControl, QueryMode, SqlIdentStyle,
    ValidatedQuery,
};
use serde_json::Value as JsonValue;

use super::catalog;
use super::query;
use super::SqliteSession;
use mystique_connector_api::ConnectorResult;

pub(super) async fn distinct_column_values(
    session: &SqliteSession,
    input: &DistinctColumnValuesInput,
) -> ConnectorResult<Vec<JsonValue>> {
    let described = catalog::describe_table(
        session,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    )
    .await?;
    let sql = distinct_values_sql(
        SqlIdentStyle::Postgres,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
        &input.column,
        &described,
        input.limit,
    )?;
    let result = query::execute_query(
        session,
        &ValidatedQuery {
            text: sql,
            mode: QueryMode::Read,
            max_rows: input.limit,
            timeout_ms: 10_000,
            collection: None,
            database: input.database.clone(),
            control: QueryControl::new(),
            params: Vec::new(),
        },
    )
    .await?;
    Ok(first_cells(result.rows))
}

fn first_cells(rows: Vec<Vec<JsonValue>>) -> Vec<JsonValue> {
    let mut values = Vec::new();
    for row in rows {
        let Some(value) = row.into_iter().next() else {
            continue;
        };
        if !value.is_null() {
            values.push(value);
        }
    }
    values
}
