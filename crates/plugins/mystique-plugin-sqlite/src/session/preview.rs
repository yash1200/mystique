//! Paged table preview (sort, filter, count).

use std::time::Instant;

use mystique_connector_api::{
    preview_sort_sql, ConnectorError, ConnectorResult, FetchTablePreviewInput, TablePreviewResult,
    TablePreviewStats,
};
use mystique_sql::filters::sql_preview_where;
use mystique_sql::{quote_ident, SqlDialect};
use serde_json::Value as JsonValue;

use std::sync::Arc;

use super::catalog;
use super::encode::value_ref_json;
use super::interrupt::{self, map_err};
use super::query_failed;
use super::SqliteSession;

pub(super) async fn fetch_table_preview(
    session: &SqliteSession,
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
    let target = quote_ident(SqlDialect::Sqlite, &input.table);
    let order = preview_sort_sql(
        SqlDialect::Sqlite,
        &columns,
        input.sort_column.as_deref(),
        input.sort_dir.as_deref(),
    )
    .unwrap_or_else(|| {
        let pks: Vec<String> = columns
            .iter()
            .filter(|c| c.pk)
            .map(|c| quote_ident(SqlDialect::Sqlite, &c.name))
            .collect();
        if !pks.is_empty() {
            format!(" ORDER BY {}", pks.join(", "))
        } else {
            " ORDER BY rowid".into()
        }
    });
    let limit = input.max_rows;
    let select_list = if columns.is_empty() {
        "*".into()
    } else {
        columns
            .iter()
            .map(|c| quote_ident(SqlDialect::Sqlite, &c.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let offset = input.offset;
    let where_sql = sql_preview_where(
        SqlDialect::Sqlite,
        &input.filters,
        input.code_filter.as_deref(),
    )?;
    let sql = format!(
        "SELECT {select_list} FROM {target}{where_sql}{order} LIMIT {limit} OFFSET {offset}"
    );
    let pool = session.pool();
    let max_rows = input.max_rows as usize;
    let control = Arc::clone(&input.control);
    let result = session
        .blocking(move || -> ConnectorResult<Vec<Vec<JsonValue>>> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            interrupt::arm(&conn, &control);
            let _settle = interrupt::Settle {
                conn: &conn,
                control: &control,
            };
            let mut stmt = conn.prepare(&sql).map_err(|err| map_err(&control, err))?;
            let col_count = stmt.column_count();
            let mut rows_iter = stmt.query([]).map_err(|err| map_err(&control, err))?;
            let mut out_rows = Vec::new();
            while let Some(row) = rows_iter.next().map_err(|err| map_err(&control, err))? {
                if control.is_cancelled() {
                    return Err(ConnectorError::Query("Query cancelled.".into()));
                }
                if out_rows.len() >= max_rows {
                    break;
                }
                let mut cells = Vec::with_capacity(col_count);
                for j in 0..col_count {
                    let v = row.get_ref(j).map_err(|err| map_err(&control, err))?;
                    cells.push(value_ref_json(v));
                }
                out_rows.push(cells);
            }
            Ok(out_rows)
        })
        .await?;

    let row_count = result.len() as u64;
    let truncated = row_count >= u64::from(input.max_rows);
    Ok(TablePreviewResult {
        engine: "sqlite".into(),
        columns,
        rows: result,
        row_count,
        total_rows: 0,
        total_known: false,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

pub(super) async fn fetch_table_preview_stats(
    session: &SqliteSession,
    input: &FetchTablePreviewInput,
) -> ConnectorResult<TablePreviewStats> {
    session.ensure_open()?;
    let target = quote_ident(SqlDialect::Sqlite, &input.table);
    let where_sql = sql_preview_where(
        SqlDialect::Sqlite,
        &input.filters,
        input.code_filter.as_deref(),
    )?;
    let count_sql = format!("SELECT COUNT(*) FROM {target}{where_sql}");
    let pool = session.pool();
    let total_rows = session
        .blocking(move || -> ConnectorResult<u64> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            let total: i64 = conn
                .query_row(&count_sql, [], |r| r.get(0))
                .map_err(query_failed)?;
            Ok(total.max(0) as u64)
        })
        .await?;
    Ok(TablePreviewStats {
        total_rows,
        total_known: true,
        references: Vec::new(),
    })
}
