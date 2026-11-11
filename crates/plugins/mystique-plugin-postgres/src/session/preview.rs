//! Paged table preview (sort, filter, count).

use std::time::Instant;

use mystique_connector_api::{
    preview_sort_sql, ConnectorError, ConnectorResult, FetchTablePreviewInput,
    TablePreviewColumnRef, TablePreviewResult, TablePreviewStats,
};
use mystique_sql::filters::sql_preview_where;
use mystique_sql::{qualify_table, SqlDialect};
use serde_json::Value as JsonValue;

use super::catalog;
use super::encode::{cell_json, preview_order_sql, preview_select_list};
use super::PostgresSession;
use super::{pg_col, query_failed};

pub(super) async fn fetch_table_preview(
    session: &PostgresSession,
    input: &FetchTablePreviewInput,
) -> ConnectorResult<TablePreviewResult> {
    session.ensure_open()?;
    let start = Instant::now();
    let columns = catalog::describe_columns(
        session,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    )
    .await?;
    let target = qualify_table(
        SqlDialect::Postgres,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let order = preview_sort_sql(
        SqlDialect::Postgres,
        &columns,
        input.sort_column.as_deref(),
        input.sort_dir.as_deref(),
    )
    .unwrap_or_else(|| preview_order_sql(SqlDialect::Postgres, &columns));
    let limit = input.max_rows as i64;
    let select_list = preview_select_list(SqlDialect::Postgres, &columns);
    let offset = input.offset;
    let where_sql = sql_preview_where(
        SqlDialect::Postgres,
        &input.filters,
        input.code_filter.as_deref(),
    )?;
    let sql = format!(
        "SELECT {select_list} FROM {target}{where_sql}{order} LIMIT {limit} OFFSET {offset}"
    );

    let client = session.run_client(&input.control).await?;
    let rows = client
        .query(sql.as_str(), &[])
        .await
        .map_err(query_failed)?;

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
    let truncated = row_count >= u64::from(input.max_rows);
    Ok(TablePreviewResult {
        engine: "postgres".into(),
        columns,
        rows: out_rows,
        row_count,
        total_rows: 0,
        total_known: false,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
        next_page_token: None,
    })
}

pub(super) async fn fetch_table_preview_stats(
    session: &PostgresSession,
    input: &FetchTablePreviewInput,
) -> ConnectorResult<TablePreviewStats> {
    session.ensure_open()?;
    let target = qualify_table(
        SqlDialect::Postgres,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let where_sql = sql_preview_where(
        SqlDialect::Postgres,
        &input.filters,
        input.code_filter.as_deref(),
    )?;
    let count_sql = format!("SELECT COUNT(*) FROM {target}{where_sql}");
    let count_fut = async {
        // Cancelled up front when the manager skips the count; interrupted
        // by the budget or the grid otherwise. FK targets load either way.
        if input.control.is_cancelled() {
            return Err(ConnectorError::Query("Count skipped.".into()));
        }
        let client = session.run_client(&input.control).await?;
        let count_row = client
            .query_one(&count_sql, &[])
            .await
            .map_err(query_failed)?;
        let total: i64 = pg_col(&count_row, 0)?;
        ConnectorResult::Ok(total.max(0) as u64)
    };
    let refs_fut = catalog::table_references(session, input.schema.as_deref(), &input.table);
    let (count, refs) = tokio::join!(count_fut, refs_fut);
    let total_known = count.is_ok();
    Ok(TablePreviewStats {
        total_rows: count.unwrap_or(0),
        total_known,
        references: refs
            .unwrap_or_default()
            .into_iter()
            .map(|(column, references)| TablePreviewColumnRef { column, references })
            .collect(),
    })
}

/// The planner's row estimate for the same table and filters:
/// `EXPLAIN (FORMAT JSON) SELECT 1 FROM … WHERE …`. It scales `reltuples` by
/// the table's current size, so it tracks growth since the last `ANALYZE`.
pub(super) async fn estimate_table_rows(
    session: &PostgresSession,
    input: &FetchTablePreviewInput,
) -> ConnectorResult<Option<u64>> {
    session.ensure_open()?;
    let target = qualify_table(
        SqlDialect::Postgres,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let where_sql = sql_preview_where(
        SqlDialect::Postgres,
        &input.filters,
        input.code_filter.as_deref(),
    )?;
    let sql = format!("EXPLAIN (FORMAT JSON) SELECT 1 FROM {target}{where_sql}");
    let client = session.client().await?;
    let row = client
        .query_one(sql.as_str(), &[])
        .await
        .map_err(query_failed)?;
    let plan: JsonValue = pg_col(&row, 0)?;
    Ok(plan_rows(&plan))
}

/// `[{"Plan": {"Plan Rows": n, …}}]` → `n`.
fn plan_rows(plan: &JsonValue) -> Option<u64> {
    let rows = plan.get(0)?.get("Plan")?.get("Plan Rows")?.as_f64()?;
    (rows.is_finite() && rows >= 0.0).then(|| rows.round() as u64)
}

#[cfg(test)]
mod tests {
    use super::plan_rows;
    use serde_json::json;

    #[test]
    fn plan_rows_reads_the_top_node() {
        let plan = json!([{ "Plan": { "Node Type": "Seq Scan", "Plan Rows": 1234.0 } }]);
        assert_eq!(plan_rows(&plan), Some(1234));
        assert_eq!(plan_rows(&json!([{ "Plan": {} }])), None);
        assert_eq!(plan_rows(&json!({})), None);
    }
}
