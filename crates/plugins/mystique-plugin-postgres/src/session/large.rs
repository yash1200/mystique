//! Re-read one clipped cell by primary key.

use mystique_connector_api::{
    large_cell_select, page_cell, ConnectorResult, FetchLargeValueInput, FetchLargeValueResult,
    SqlIdentStyle, SqlPlaceholder, SqlTextCast,
};
use tokio_postgres::types::ToSql;

use super::encode::cell_json;
use super::query_failed;
use super::PostgresSession;

pub(super) async fn fetch_large_value(
    session: &PostgresSession,
    input: &FetchLargeValueInput,
) -> ConnectorResult<FetchLargeValueResult> {
    session.ensure_open()?;
    let select = large_cell_select(
        SqlIdentStyle::Postgres,
        SqlPlaceholder::Dollar,
        SqlTextCast::Text,
        &input.locator,
    )?;
    let client = session.client().await?;
    let _guard = session.arm_cancel(&client, &input.control);
    let params: Vec<&(dyn ToSql + Sync)> = select
        .params
        .iter()
        .map(|value| value as &(dyn ToSql + Sync))
        .collect();
    let row = client
        .query_opt(select.sql.as_str(), &params)
        .await
        .map_err(query_failed)?;
    let Some(row) = row else {
        return Err(mystique_connector_api::ConnectorError::Query(
            "No row matched that key.".into(),
        ));
    };
    if input.control.is_cancelled() {
        return Err(mystique_connector_api::ConnectorError::Query(
            "Query cancelled.".into(),
        ));
    }
    Ok(page_cell(&cell_json(&row, 0), input.offset, input.limit))
}
