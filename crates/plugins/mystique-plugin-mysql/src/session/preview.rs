//! Paged table preview (sort, filter, count).

use std::time::Instant;

use mysql_async::prelude::*;
use mysql_async::Value;
use mystique_connector_api::{
    preview_sort_sql, ConnectorResult, FetchTablePreviewInput, TablePreviewColumnRef,
    TablePreviewResult, TablePreviewStats,
};
use mystique_sql::filters::sql_preview_where;
use mystique_sql::{qualify_table, quote_ident, SqlDialect};
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
        SqlDialect::Mysql,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let order = preview_sort_sql(
        SqlDialect::Mysql,
        &columns,
        input.sort_column.as_deref(),
        input.sort_dir.as_deref(),
    )
    .unwrap_or_else(|| {
        let pks: Vec<String> = columns
            .iter()
            .filter(|c| c.pk)
            .map(|c| quote_ident(SqlDialect::Mysql, &c.name))
            .collect();
        if !pks.is_empty() {
            format!(" ORDER BY {}", pks.join(", "))
        } else if let Some(first) = columns.first() {
            format!(" ORDER BY {}", quote_ident(SqlDialect::Mysql, &first.name))
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
            .map(|c| quote_ident(SqlDialect::Mysql, &c.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let offset = input.offset;
    let where_sql = sql_preview_where(
        SqlDialect::Mysql,
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
        SqlDialect::Mysql,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let where_sql = sql_preview_where(
        SqlDialect::Mysql,
        &input.filters,
        input.code_filter.as_deref(),
    )?;
    let count_sql = format!("SELECT COUNT(*) FROM {target}{where_sql}");
    let count_fut = async {
        // Cancelled up front when the manager skips the count; stopped by
        // the budget or the grid otherwise. FK targets load either way.
        if input.control.is_cancelled() {
            return Err(mystique_connector_api::ConnectorError::Query(
                "Count skipped.".into(),
            ));
        }
        let mut conn = session.conn().await?;
        arm_kill(&session.pool(), conn.id(), &input.control);
        let guard = InterruptGuard::new(&input.control);
        let total = crate::rows::query_first_opt::<u64>(&mut conn, &count_sql).await;
        drop(guard);
        if input.control.is_cancelled() {
            let _ = conn.disconnect().await;
            return Err(mystique_connector_api::ConnectorError::Query(
                "Query cancelled.".into(),
            ));
        }
        ConnectorResult::Ok(total?.unwrap_or(0))
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

/// The optimizer's row estimate for the same table and filters:
/// `EXPLAIN FORMAT=JSON SELECT 1 FROM … WHERE …`.
pub(super) async fn estimate_table_rows(
    session: &MysqlSession,
    input: &FetchTablePreviewInput,
) -> ConnectorResult<Option<u64>> {
    session.ensure_open()?;
    let target = qualify_table(
        SqlDialect::Mysql,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let where_sql = sql_preview_where(
        SqlDialect::Mysql,
        &input.filters,
        input.code_filter.as_deref(),
    )?;
    let sql = format!("EXPLAIN FORMAT=JSON SELECT 1 FROM {target}{where_sql}");
    let mut conn = session.conn().await?;
    let Some(text) = crate::rows::query_first_opt::<String>(&mut conn, &sql).await? else {
        return Ok(None);
    };
    Ok(serde_json::from_str::<JsonValue>(&text)
        .ok()
        .and_then(|plan| explain_rows(&plan)))
}

/// Rows the plan produces after the WHERE. Format version 2 (MySQL 9+,
/// `explain_json_format_version = 2`) has `query_plan.estimated_rows`;
/// version 1 has one table under `query_block`.
fn explain_rows(plan: &JsonValue) -> Option<u64> {
    let number = |v: &JsonValue| v.as_f64().or_else(|| v.as_str()?.parse().ok());
    if let Some(rows) = plan
        .get("query_plan")
        .and_then(|top| top.get("estimated_rows"))
        .and_then(number)
    {
        return Some(rows.max(0.0).round() as u64);
    }
    let table = plan.get("query_block")?.get("table")?;
    if let Some(rows) = table.get("rows_produced_per_join").and_then(number) {
        return Some(rows.max(0.0).round() as u64);
    }
    let scanned = table.get("rows_examined_per_scan").and_then(number)?;
    let filtered = table.get("filtered").and_then(number).unwrap_or(100.0);
    Some((scanned * filtered / 100.0).max(0.0).round() as u64)
}

#[cfg(test)]
mod estimate_tests {
    use super::explain_rows;
    use serde_json::json;

    #[test]
    fn explain_rows_reads_the_single_table() {
        let plan = json!({ "query_block": { "table": {
            "rows_examined_per_scan": 1000, "rows_produced_per_join": 100, "filtered": "10.00"
        } } });
        assert_eq!(explain_rows(&plan), Some(100));
        let older = json!({ "query_block": { "table": {
            "rows_examined_per_scan": 1000, "filtered": "25.00"
        } } });
        assert_eq!(explain_rows(&older), Some(250));
        let impossible = json!({ "query_block": { "message": "Impossible WHERE" } });
        assert_eq!(explain_rows(&impossible), None);
        let v2 = json!({ "query_plan": { "operation": "Filter", "estimated_rows": 42.4,
            "inputs": [{ "estimated_rows": 1000.0 }] } });
        assert_eq!(explain_rows(&v2), Some(42));
    }
}
