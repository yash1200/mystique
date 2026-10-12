//! Policy-validated SQL execution for a pooled Postgres session.

use std::time::Instant;

use mystique_connector_api::{
    split_sql_statements_for, sql_explain_executes, ConnectorError, ConnectorResult, QueryColumn,
    QueryMode, QueryResult, SqlDialect, ValidatedQuery,
};
use tokio_postgres::Client;

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

    let stmts = split_sql_statements_for(&query.text, SqlDialect::Postgres);
    if stmts.is_empty() {
        return Err(ConnectorError::Query("Query is empty.".into()));
    }

    let timeout = query.timeout_ms.max(1);
    // Read mode, and any EXPLAIN that executes its target, must not be able to
    // write even when the classifier misses (data-modifying CTE, parenthesised
    // ANALYZE). ROLLBACK before the connection returns to the pool. SET LOCAL
    // dies with that transaction, so statement_timeout does not leak.
    let lock_writes = query.mode == QueryMode::Read
        || stmts
            .iter()
            .any(|sql| sql_explain_executes(sql, SqlDialect::Postgres));

    let ran = if lock_writes {
        client
            .batch_execute("BEGIN READ ONLY")
            .await
            .map_err(query_failed)?;
        if let Err(err) = client
            .batch_execute(&format!("SET LOCAL statement_timeout = {timeout}"))
            .await
        {
            return end_read_only_txn(client, Err(query_failed(err))).await;
        }
        let result = run_batch(&client, query, &stmts, true).await;
        end_read_only_txn(client, result).await?
    } else {
        let _ = client
            .batch_execute(&format!("SET statement_timeout = {timeout}"))
            .await;
        let result = run_batch(&client, query, &stmts, false).await;
        let _ = client.batch_execute("RESET statement_timeout").await;
        result?
    };

    let mut out = ran;
    out.duration_ms = start.elapsed().as_millis() as u64;
    Ok(out)
}

async fn end_read_only_txn(
    client: deadpool_postgres::Object,
    result: ConnectorResult<QueryResult>,
) -> ConnectorResult<QueryResult> {
    match client.batch_execute("ROLLBACK").await {
        Ok(()) => result,
        Err(err) => {
            // A failed ROLLBACK would hand the next checkout an open transaction.
            drop(deadpool_postgres::Object::take(client));
            result.and(Err(query_failed(err)))
        }
    }
}

async fn run_batch(
    client: &Client,
    query: &ValidatedQuery,
    stmts: &[String],
    locked: bool,
) -> ConnectorResult<QueryResult> {
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
            let slice = rows.get(..take).unwrap_or(&[]);

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
        // COMMIT inside the batch would otherwise let the next statement write.
        if locked {
            ensure_still_read_only(client).await?;
        }
    }

    last.ok_or_else(|| ConnectorError::Query("No statements to run.".into()))
}

async fn ensure_still_read_only(client: &Client) -> ConnectorResult<()> {
    let row = client
        .query_one("SHOW transaction_read_only", &[])
        .await
        .map_err(query_failed)?;
    let value: String = row.try_get(0).map_err(query_failed)?;
    if value.eq_ignore_ascii_case("on") {
        return Ok(());
    }
    Err(ConnectorError::Query(
        "The read-only transaction ended before every statement ran. Later statements were not executed.".into(),
    ))
}
