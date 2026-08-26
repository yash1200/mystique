//! Paged table preview (sort, filter, count).

use std::time::Instant;

use mysql_async::prelude::*;
use mysql_async::Value;
use mystique_connector_api::{
    preview_sort_sql, qualify_table, quote_ident, sql_preview_where, ConnectorError,
    ConnectorResult, FetchTablePreviewInput, SqlIdentStyle, TablePreviewResult,
};
use serde_json::Value as JsonValue;

use super::catalog;
use super::encode::value_json;
use super::MysqlSession;

pub(super) async fn fetch_table_preview(
    session: &MysqlSession,
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
        SqlIdentStyle::Mysql,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let order = preview_sort_sql(
        SqlIdentStyle::Mysql,
        &columns,
        input.sort_column.as_deref(),
        input.sort_dir.as_deref(),
    )
    .unwrap_or_else(|| {
        let pks: Vec<String> = columns
            .iter()
            .filter(|c| c.pk)
            .map(|c| quote_ident(SqlIdentStyle::Mysql, &c.name))
            .collect();
        if !pks.is_empty() {
            format!(" ORDER BY {}", pks.join(", "))
        } else if let Some(first) = columns.first() {
            format!(
                " ORDER BY {}",
                quote_ident(SqlIdentStyle::Mysql, &first.name)
            )
        } else {
            String::new()
        }
    });
    let limit = input.max_rows;
    let select_list = if columns.is_empty() {
        "*".into()
    } else {
        columns
            .iter()
            .map(|c| quote_ident(SqlIdentStyle::Mysql, &c.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let offset = input.offset;
    let where_sql = sql_preview_where(
        SqlIdentStyle::Mysql,
        &input.filters,
        input.code_filter.as_deref(),
    )?;
    let sql = format!(
        "SELECT {select_list} FROM {target}{where_sql}{order} LIMIT {limit} OFFSET {offset}"
    );

    let mut conn = session.conn().await?;
    let total_rows: u64 = conn
        .query_first(format!("SELECT COUNT(*) FROM {target}{where_sql}"))
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?
        .unwrap_or(0);
    let result_set = conn
        .query_iter(sql.as_str())
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;

    let col_count = result_set.columns_ref().len();
    let mut out_rows: Vec<Vec<JsonValue>> = Vec::new();
    let mut result_set = result_set;
    while let Some(row) = result_set
        .next()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?
    {
        if out_rows.len() >= input.max_rows as usize {
            break;
        }
        let mut cells = Vec::with_capacity(col_count);
        for j in 0..col_count {
            let v: Value = row.as_ref(j).cloned().unwrap_or(Value::NULL);
            cells.push(value_json(v));
        }
        out_rows.push(cells);
    }

    let row_count = out_rows.len() as u64;
    let truncated = input.offset as u64 + row_count < total_rows;
    Ok(TablePreviewResult {
        engine: "mysql".into(),
        columns,
        rows: out_rows,
        row_count,
        total_rows,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}
