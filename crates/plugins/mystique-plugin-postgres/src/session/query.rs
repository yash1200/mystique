//! Policy-validated SQL execution for a pooled Postgres session.

use std::time::Instant;

use mystique_connector_api::{
    split_sql_statements, ConnectorError, ConnectorResult, QueryColumn, QueryMode, QueryResult,
    ValidatedQuery,
};

use super::encode::cell_json;
use super::query_failed;
use super::PostgresSession;

pub(super) async fn execute_query(
    session: &PostgresSession,
    query: &ValidatedQuery,
) -> ConnectorResult<QueryResult> {
    session.ensure_open()?;

    // Multi-statement batches: run each in order; return only the last result.
    let start = Instant::now();
    let client = session.client().await?;

    let timeout = query.timeout_ms.max(1);
    let _ = client
        .batch_execute(&format!("SET statement_timeout = {timeout}"))
        .await;

    let stmts = split_sql_statements(&query.text);
    if stmts.is_empty() {
        return Err(ConnectorError::Query("Query is empty.".into()));
    }

    let limit = query.max_rows as usize;
    let mut last: Option<QueryResult> = None;
    let batch_n = stmts.len();

    for (i, sql) in stmts.iter().enumerate() {
        let lower = sql.to_ascii_lowercase();
        let is_select_like = lower.starts_with("select")
            || lower.starts_with("with")
            || lower.starts_with("values")
            || lower.starts_with("table")
            || lower.starts_with("show")
            || lower.starts_with("explain");

        let result = if is_select_like {
            let rows = client
                .query(sql.as_str(), &[])
                .await
                .map_err(query_failed)?;

            let truncated = rows.len() > limit;
            let take = rows.len().min(limit);
            let slice = &rows[..take];

            let columns: Vec<QueryColumn> = if let Some(first) = slice.first() {
                first
                    .columns()
                    .iter()
                    .map(|c| QueryColumn {
                        name: c.name().to_string(),
                        data_type: Some(c.type_().name().to_string()),
                    })
                    .collect()
            } else {
                Vec::new()
            };

            let mut out_rows = Vec::with_capacity(take);
            for row in slice {
                let mut cells = Vec::with_capacity(row.len());
                for j in 0..row.len() {
                    cells.push(cell_json(row, j));
                }
                out_rows.push(cells);
            }

            let mut message = if truncated {
                Some(format!("Result truncated to {limit} rows."))
            } else {
                None
            };
            if batch_n > 1 && i + 1 == batch_n {
                let note = format!("Last of {batch_n} statements.");
                message = Some(match message {
                    Some(m) => format!("{m} {note}"),
                    None => note,
                });
            }

            QueryResult {
                engine: "postgres".into(),
                mode: query.mode,
                columns,
                row_count: out_rows.len() as u64,
                rows: out_rows,
                truncated,
                duration_ms: 0,
                message,
            }
        } else {
            client
                .batch_execute(sql.as_str())
                .await
                .map_err(query_failed)?;

            let mut message = Some(format!(
                "Statement executed ({}). Tabular rows only for SELECT-like queries.",
                match query.mode {
                    QueryMode::Read => "read",
                    QueryMode::Write => "write",
                }
            ));
            if batch_n > 1 && i + 1 == batch_n {
                message = Some(format!(
                    "{}; last of {batch_n} statements.",
                    message.unwrap_or_default().trim_end_matches('.')
                ));
            }

            QueryResult {
                engine: "postgres".into(),
                mode: query.mode,
                columns: Vec::new(),
                row_count: 0,
                rows: Vec::new(),
                truncated: false,
                duration_ms: 0,
                message,
            }
        };

        last = Some(result);
    }

    let mut out = last.ok_or_else(|| ConnectorError::Query("No statements to run.".into()))?;
    out.duration_ms = start.elapsed().as_millis() as u64;
    Ok(out)
}
