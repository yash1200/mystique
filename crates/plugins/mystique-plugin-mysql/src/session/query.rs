//! Policy-validated SQL execution for a pooled MySQL session.

use std::time::Instant;

use mysql_async::prelude::*;
use mysql_async::Value;
use mystique_connector_api::{
    split_sql_statements_for, sql_explain_executes, ConnectorError, ConnectorResult, QueryColumn,
    QueryMode, QueryResult, SqlDialect, ValidatedQuery,
};
use serde_json::Value as JsonValue;

use super::encode::{mysql_type_label, value_json};
use super::query_failed;
use super::MysqlSession;

pub(super) async fn execute_query(
    session: &MysqlSession,
    query: &ValidatedQuery,
) -> ConnectorResult<QueryResult> {
    session.ensure_open()?;

    // Multi-statement batches: run each in order; return only the last result.
    let start = Instant::now();
    let mut conn = session.conn().await?;

    let stmts = split_sql_statements_for(&query.text, SqlDialect::Mysql);
    if stmts.is_empty() {
        return Err(ConnectorError::Query("Query is empty.".into()));
    }

    let timeout = query.timeout_ms.max(1);
    // Same lock as Postgres: Read mode and EXPLAIN ANALYZE cannot write even
    // when the classifier misses. A rejected START TRANSACTION fails the query
    // closed — we do not fall through to an unconstrained connection.
    let lock_writes = query.mode == QueryMode::Read
        || stmts
            .iter()
            .any(|sql| sql_explain_executes(sql, SqlDialect::Mysql));

    let _ = conn
        .query_drop(format!("SET SESSION max_execution_time = {timeout}"))
        .await;

    let ran = if lock_writes {
        if let Err(err) = conn.query_drop("START TRANSACTION READ ONLY").await {
            let _ = conn.query_drop("SET SESSION max_execution_time = 0").await;
            return Err(query_failed(err));
        }
        let result = run_batch(&mut conn, query, &stmts, true).await;
        end_read_only_txn(conn, result).await?
    } else {
        let result = run_batch(&mut conn, query, &stmts, false).await;
        let _ = conn.query_drop("SET SESSION max_execution_time = 0").await;
        result?
    };

    let mut out = ran;
    out.duration_ms = start.elapsed().as_millis() as u64;
    Ok(out)
}

async fn end_read_only_txn(
    mut conn: mysql_async::Conn,
    result: ConnectorResult<QueryResult>,
) -> ConnectorResult<QueryResult> {
    let rolled_back = conn.query_drop("ROLLBACK").await;
    let _ = conn.query_drop("SET SESSION max_execution_time = 0").await;
    match rolled_back {
        Ok(()) => result,
        Err(err) => {
            let _ = conn.disconnect().await;
            result.and(Err(query_failed(err)))
        }
    }
}

async fn run_batch(
    conn: &mut mysql_async::Conn,
    query: &ValidatedQuery,
    stmts: &[String],
    locked: bool,
) -> ConnectorResult<QueryResult> {
    let max_rows = query.max_rows as usize;
    let mode = query.mode;
    let batch_n = stmts.len();
    let mut last: Option<QueryResult> = None;

    for (i, sql) in stmts.iter().enumerate() {
        let lower = sql.to_ascii_lowercase();
        let is_select_like = lower.starts_with("select")
            || lower.starts_with("with")
            || lower.starts_with("show")
            || lower.starts_with("describe")
            || lower.starts_with("desc")
            || lower.starts_with("explain")
            || lower.starts_with("values");

        let result = if is_select_like {
            let result_set = conn.query_iter(sql.as_str()).await.map_err(query_failed)?;

            let columns: Vec<QueryColumn> = result_set
                .columns_ref()
                .iter()
                .map(|c| QueryColumn {
                    name: c.name_str().into_owned(),
                    data_type: Some(mysql_type_label(c.column_type()).to_string()),
                })
                .collect();

            let mut out_rows: Vec<Vec<JsonValue>> = Vec::new();
            let mut truncated = false;
            let mut result_set = result_set;
            while let Some(row) = result_set.next().await.map_err(query_failed)? {
                if out_rows.len() >= max_rows {
                    truncated = true;
                    break;
                }
                let mut cells = Vec::with_capacity(columns.len());
                for j in 0..columns.len() {
                    let v: Value = row.as_ref(j).cloned().unwrap_or(Value::NULL);
                    cells.push(value_json(v));
                }
                out_rows.push(cells);
            }
            while result_set.next().await.map_err(query_failed)?.is_some() {
                truncated = true;
            }

            let mut message = if truncated {
                Some(format!("Result truncated to {max_rows} rows."))
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
                engine: "mysql".into(),
                mode,
                columns,
                row_count: out_rows.len() as u64,
                rows: out_rows,
                truncated,
                duration_ms: 0,
                message,
            }
        } else {
            conn.query_drop(sql.as_str()).await.map_err(query_failed)?;

            let mut message = Some(format!(
                "Statement executed ({}). Tabular rows only for SELECT-like queries.",
                match mode {
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
                engine: "mysql".into(),
                mode,
                columns: Vec::new(),
                row_count: 0,
                rows: Vec::new(),
                truncated: false,
                duration_ms: 0,
                message,
            }
        };

        last = Some(result);
        if locked {
            ensure_still_read_only(conn).await?;
        }
    }

    last.ok_or_else(|| ConnectorError::Query("No statements to run.".into()))
}

async fn ensure_still_read_only(conn: &mut mysql_async::Conn) -> ConnectorResult<()> {
    // Cast so a numeric or string column both come back as text.
    let value = crate::rows::query_first_opt::<String>(
        conn,
        "SELECT CASE WHEN @@transaction_read_only = 1 THEN 'on' ELSE 'off' END",
    )
    .await?;
    if value.as_deref() == Some("on") {
        return Ok(());
    }
    Err(ConnectorError::Query(
        "The read-only transaction ended before every statement ran. Later statements were not executed.".into(),
    ))
}
