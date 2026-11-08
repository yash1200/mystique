//! Paged table preview (sort, filter, count).

use std::time::Instant;

use mysql_async::prelude::*;
use mysql_async::Value;
use mystique_connector_api::{
    preview_sort_sql, qualify_table, quote_ident, sql_preview_where, ConnectorResult,
    FetchTablePreviewInput, SqlIdentStyle, TablePreviewColumnRef, TablePreviewResult,
    TablePreviewStats,
};
use serde_json::Value as JsonValue;

use mystique_connector_api::{InterruptGuard, FETCH_TIMEOUT_MS};

use super::cancel::{apply_timeouts, arm_kill, clear_timeouts};
use super::catalog;
use super::encode::value_json;
use super::query_failed;
use super::MysqlSession;

pub(super) async fn fetch_table_preview(
    session: &MysqlSession,
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
    arm_kill(&session.pool(), conn.id(), &input.control);
    let _guard = InterruptGuard::new(&input.control);
    apply_timeouts(&mut conn, FETCH_TIMEOUT_MS).await;
    let mut out_rows: Vec<Vec<JsonValue>> = Vec::new();
    let mut unread = false;
    let mut failure = None;
    match conn.query_iter(sql.as_str()).await {
        Err(err) => {
            failure = Some(if input.control.is_cancelled() {
                mystique_connector_api::ConnectorError::Query("Query cancelled.".into())
            } else {
                query_failed(err)
            });
        }
        Ok(mut result_set) => {
            let col_count = result_set.columns_ref().len();
            loop {
                if input.control.is_cancelled() {
                    unread = true;
                    failure = Some(mystique_connector_api::ConnectorError::Query(
                        "Query cancelled.".into(),
                    ));
                    break;
                }
                match result_set.next().await {
                    Ok(None) => break,
                    Err(err) => {
                        unread = true;
                        failure = Some(if input.control.is_cancelled() {
                            mystique_connector_api::ConnectorError::Query("Query cancelled.".into())
                        } else {
                            query_failed(err)
                        });
                        break;
                    }
                    Ok(Some(row)) => {
                        if out_rows.len() >= input.max_rows as usize {
                            unread = true;
                            break;
                        }
                        let mut cells = Vec::with_capacity(col_count);
                        for j in 0..col_count {
                            let v: Value = row.as_ref(j).cloned().unwrap_or(Value::NULL);
                            cells.push(value_json(v));
                        }
                        out_rows.push(cells);
                    }
                }
            }
        }
    }
    input.control.clear_interrupt();
    if unread || input.control.is_cancelled() {
        let _ = conn.disconnect().await;
    } else {
        clear_timeouts(&mut conn).await;
    }
    if let Some(err) = failure {
        return Err(err);
    }

    let row_count = out_rows.len() as u64;
    let truncated = row_count >= u64::from(input.max_rows);
    Ok(TablePreviewResult {
        engine: "mysql".into(),
        columns,
        rows: out_rows,
        row_count,
        total_rows: 0,
        total_known: false,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

pub(super) async fn fetch_table_preview_stats(
    session: &MysqlSession,
    input: &FetchTablePreviewInput,
) -> ConnectorResult<TablePreviewStats> {
    session.ensure_open()?;
    let target = qualify_table(
        SqlIdentStyle::Mysql,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let where_sql = sql_preview_where(
        SqlIdentStyle::Mysql,
        &input.filters,
        input.code_filter.as_deref(),
    )?;
    let count_sql = format!("SELECT COUNT(*) FROM {target}{where_sql}");
    let count_fut = async {
        let mut conn = session.conn().await?;
        let total = crate::rows::query_first_opt::<u64>(&mut conn, &count_sql)
            .await?
            .unwrap_or(0);
        ConnectorResult::Ok(total)
    };
    let refs_fut = catalog::table_references(
        session,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
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
