//! Paged table preview (sort, filter, count).

use std::time::Instant;

use mystique_connector_api::{
    preview_sort_sql, qualify_table, sql_preview_where, ConnectorResult, FetchTablePreviewInput,
    SqlIdentStyle, TablePreviewResult,
};

use super::catalog;
use super::encode::{cell_json, preview_order_sql, preview_select_list};
use super::query_failed;
use super::PostgresSession;

pub(super) async fn fetch_table_preview(
    session: &PostgresSession,
    input: &FetchTablePreviewInput,
) -> ConnectorResult<TablePreviewResult> {
    session.ensure_open()?;
    let start = Instant::now();
    let columns = catalog::describe_table(
        session,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    )
    .await?;
    let target = qualify_table(
        SqlIdentStyle::Postgres,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let order = preview_sort_sql(
        SqlIdentStyle::Postgres,
        &columns,
        input.sort_column.as_deref(),
        input.sort_dir.as_deref(),
    )
    .unwrap_or_else(|| preview_order_sql(SqlIdentStyle::Postgres, &columns));
    let limit = input.max_rows as i64;
    let select_list = preview_select_list(SqlIdentStyle::Postgres, &columns);
    let offset = input.offset;
    let where_sql = sql_preview_where(
        SqlIdentStyle::Postgres,
        &input.filters,
        input.code_filter.as_deref(),
    )?;
    let sql = format!(
        "SELECT {select_list} FROM {target}{where_sql}{order} LIMIT {limit} OFFSET {offset}"
    );

    let client = session.client().await?;
    let total_rows: i64 = client
        .query_one(&format!("SELECT COUNT(*) FROM {target}{where_sql}"), &[])
        .await
        .map_err(query_failed)?
        .get(0);
    let rows = client
        .query(sql.as_str(), &[])
        .await
        .map_err(query_failed)?;

    let truncated = (input.offset as i64) + (rows.len() as i64) < total_rows;
    let take = rows.len().min(input.max_rows as usize);
    let mut out_rows = Vec::with_capacity(take);
    for row in rows.iter().take(take) {
        let mut cells = Vec::with_capacity(row.len());
        for j in 0..row.len() {
            cells.push(cell_json(row, j));
        }
        out_rows.push(cells);
    }
    let row_count = out_rows.len() as u64;
    Ok(TablePreviewResult {
        engine: "postgres".into(),
        columns,
        rows: out_rows,
        row_count,
        total_rows: total_rows.max(0) as u64,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}
