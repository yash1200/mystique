//! Policy-validated SQL execution for a pooled MySQL session.

use std::time::Instant;

use mysql_async::prelude::*;
use mysql_async::Value;
use mystique_connector_api::{
    affected_message, emit_progress, idle_result, poll_until_idle, split_sql_statements_for,
    sql_explain_executes, BatchPhase, ConnectorError, ConnectorResult, InterruptGuard,
    PlannedStatement, QueryBatchResult, QueryColumn, QueryMode, QueryResult, SqlDialect,
    StatementResult, ValidatedBatch, ValidatedQuery,
};
use serde_json::Value as JsonValue;

use super::cancel::{apply_timeouts, arm_kill, clear_timeouts, inactivity};
use super::encode::{mysql_type_label, value_json};
use super::MysqlSession;
use super::{query_failed, query_failed_at};

pub(super) async fn execute_query(
    session: &MysqlSession,
    query: &ValidatedQuery,
) -> ConnectorResult<QueryResult> {
    session.ensure_open()?;

    // Multi-statement batches: run each in order; return only the last result.
    let start = Instant::now();
    let mut conn = session.conn().await?;
    arm_kill(&session.pool(), conn.id(), &query.control);
    let _guard = InterruptGuard::new(&query.control);

    let stmts = split_sql_statements_for(&query.text, SqlDialect::Mysql);
    if stmts.is_empty() {
        return Err(ConnectorError::Query("Query is empty.".into()));
    }

    // Same lock as Postgres: Read mode and EXPLAIN ANALYZE cannot write even
    // when the classifier misses. A rejected START TRANSACTION fails the query
    // closed — we do not fall through to an unconstrained connection.
    let lock_writes = query.mode == QueryMode::Read
        || stmts
            .iter()
            .any(|sql| sql_explain_executes(sql, SqlDialect::Mysql));

    apply_timeouts(&mut conn, query.timeout_ms).await;

    let ran = if lock_writes {
        if let Err(err) = conn.query_drop("START TRANSACTION READ ONLY").await {
            clear_timeouts(&mut conn).await;
            return Err(cancelled_or(query, err));
        }
        let result = run_batch(&mut conn, query, &stmts, true).await;
        let drop_conn = drop_mysql_conn(query, &result);
        query.control.clear_interrupt();
        release_conn(conn, result, true, drop_conn).await?
    } else {
        let result = run_batch(&mut conn, query, &stmts, false).await;
        let drop_conn = drop_mysql_conn(query, &result);
        query.control.clear_interrupt();
        release_conn(conn, result, false, drop_conn).await?
    };

    let mut out = ran;
    out.duration_ms = start.elapsed().as_millis() as u64;
    Ok(out)
}

fn cancelled_or(query: &ValidatedQuery, err: mysql_async::Error) -> ConnectorError {
    if query.control.is_cancelled() {
        ConnectorError::Query("Query cancelled.".into())
    } else {
        query_failed(err)
    }
}

/// A capped result still has unread packets. Returning the connection would
/// make the driver drain them. Disconnect instead, on both the plain path and
/// the read-only transaction path.
fn drop_mysql_conn(query: &ValidatedQuery, result: &ConnectorResult<QueryResult>) -> bool {
    // Unread packets (row cap or a cancel that aborted mid-stream) must not go
    // back to the pool. A normal SQL error is consumed and the connection is reusable.
    result.as_ref().map(|row| row.truncated).unwrap_or(false) || query.control.is_cancelled()
}

async fn release_conn<T>(
    mut conn: mysql_async::Conn,
    result: ConnectorResult<T>,
    read_only: bool,
    drop_conn: bool,
) -> ConnectorResult<T> {
    if drop_conn {
        let _ = conn.disconnect().await;
        return result;
    }
    if read_only {
        return end_read_only_txn(conn, result).await;
    }
    clear_timeouts(&mut conn).await;
    result
}

pub(super) async fn end_read_only_txn<T>(
    mut conn: mysql_async::Conn,
    result: ConnectorResult<T>,
) -> ConnectorResult<T> {
    let rolled_back = conn.query_drop("ROLLBACK").await;
    clear_timeouts(&mut conn).await;
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
            let mut saw_row = false;
            loop {
                if query.control.is_cancelled() {
                    return Err(ConnectorError::Query("Query cancelled.".into()));
                }
                // The first row uses the overall deadline (`SLEEP` included).
                // Later gaps use the inactivity budget and must not drop `next`
                // before `KILL QUERY` unblocks the driver.
                let step = if saw_row {
                    idle_result(
                        poll_until_idle(&query.control, inactivity(), result_set.next()).await,
                    )?
                } else {
                    result_set.next().await
                };
                let Some(row) = step.map_err(|err| cancelled_or(query, err))? else {
                    break;
                };
                saw_row = true;
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

        let stop = result.truncated;
        last = Some(result);
        if stop {
            break;
        }
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

pub(super) async fn execute_batch(
    session: &MysqlSession,
    batch: &ValidatedBatch,
) -> ConnectorResult<QueryBatchResult> {
    session.ensure_open()?;
    let mut conn = session.conn().await?;
    arm_kill(&session.pool(), conn.id(), &batch.control);
    let _guard = InterruptGuard::new(&batch.control);
    let lock_writes = batch.mode == QueryMode::Read || batch.explain_executes;
    apply_timeouts(&mut conn, batch.timeout_ms).await;

    let ran = if lock_writes {
        if let Err(err) = conn.query_drop("START TRANSACTION READ ONLY").await {
            clear_timeouts(&mut conn).await;
            return Err(cancelled_batch(batch, err));
        }
        let result = run_planned_batch(&mut conn, batch, true).await;
        let drop_conn = drop_mysql_batch(batch, &result);
        batch.control.clear_interrupt();
        release_conn(conn, result, true, drop_conn).await?
    } else {
        let result = run_planned_batch(&mut conn, batch, false).await;
        let drop_conn = drop_mysql_batch(batch, &result);
        batch.control.clear_interrupt();
        release_conn(conn, result, false, drop_conn).await?
    };
    Ok(ran)
}

fn drop_mysql_batch(batch: &ValidatedBatch, result: &ConnectorResult<QueryBatchResult>) -> bool {
    batch.control.is_cancelled()
        || result.as_ref().is_ok_and(|ran| {
            ran.results
                .iter()
                .any(|stmt| stmt.result.as_ref().is_some_and(|row| row.truncated))
        })
}

fn cancelled_batch(batch: &ValidatedBatch, err: mysql_async::Error) -> ConnectorError {
    if batch.control.is_cancelled() {
        ConnectorError::Query("Query cancelled.".into())
    } else {
        query_failed(err)
    }
}

fn located_batch(batch: &ValidatedBatch, sql: &str, err: mysql_async::Error) -> ConnectorError {
    if batch.control.is_cancelled() {
        ConnectorError::Query("Query cancelled.".into())
    } else {
        query_failed_at(sql, err)
    }
}

pub(super) async fn run_planned_batch(
    conn: &mut mysql_async::Conn,
    batch: &ValidatedBatch,
    locked: bool,
) -> ConnectorResult<QueryBatchResult> {
    let total = u32::try_from(batch.statements.len()).unwrap_or(u32::MAX);
    let mut results = Vec::with_capacity(batch.statements.len());
    for stmt in &batch.statements {
        if batch.control.is_cancelled() {
            return Err(ConnectorError::Query("Query cancelled.".into()));
        }
        emit_progress(&batch.progress, stmt.index, total, BatchPhase::Started);
        let (sets, truncated) = match drain_sets(conn, batch, stmt).await {
            Ok(drained) => drained,
            Err(err) => {
                if batch.control.is_cancelled() {
                    return Err(err);
                }
                results.push(StatementResult::failed(stmt, 0, &err));
                emit_progress(&batch.progress, stmt.index, total, BatchPhase::Finished);
                if let Some(progress) = &batch.progress {
                    progress.flush();
                }
                return Ok(QueryBatchResult {
                    results,
                    stopped_at: Some(stmt.index),
                });
            }
        };
        let index = stmt.index;
        results.extend(sets);
        emit_progress(&batch.progress, index, total, BatchPhase::Finished);
        // A capped result still has unread packets. Stop here and let the
        // caller disconnect. Truncation is not a statement error.
        if truncated {
            break;
        }
        if locked {
            if let Err(err) = ensure_still_read_only(conn).await {
                if let Some(last) = results.last_mut() {
                    last.error = Some(err.to_string());
                }
                if let Some(progress) = &batch.progress {
                    progress.flush();
                }
                return Ok(QueryBatchResult {
                    results,
                    stopped_at: Some(index),
                });
            }
        }
    }
    if let Some(progress) = &batch.progress {
        progress.flush();
    }
    Ok(QueryBatchResult {
        results,
        stopped_at: None,
    })
}

async fn drain_sets(
    conn: &mut mysql_async::Conn,
    batch: &ValidatedBatch,
    stmt: &PlannedStatement,
) -> ConnectorResult<(Vec<StatementResult>, bool)> {
    let mut qr = conn
        .query_iter(stmt.sql.as_str())
        .await
        .map_err(|err| located_batch(batch, &stmt.sql, err))?;
    let max_rows = batch.max_rows.max(1) as usize;
    let mut sets = Vec::new();
    let mut set_index: u32 = 0;
    let mut max_warnings: u16 = 0;
    loop {
        let started = Instant::now();
        let columns: Vec<QueryColumn> = qr
            .columns_ref()
            .iter()
            .map(|column| QueryColumn {
                name: column.name_str().into_owned(),
                data_type: Some(mysql_type_label(column.column_type()).to_string()),
            })
            .collect();
        let affected_at_start = qr.affected_rows();
        max_warnings = max_warnings.max(qr.warnings());
        let mut out_rows: Vec<Vec<JsonValue>> = Vec::new();
        let mut truncated = false;
        let mut saw_row = false;
        loop {
            if batch.control.is_cancelled() {
                return Err(ConnectorError::Query("Query cancelled.".into()));
            }
            let step = if saw_row {
                idle_result(poll_until_idle(&batch.control, inactivity(), qr.next()).await)?
            } else {
                qr.next().await
            };
            let Some(row) = step.map_err(|err| located_batch(batch, &stmt.sql, err))? else {
                break;
            };
            saw_row = true;
            if columns.is_empty() {
                continue;
            }
            if out_rows.len() >= max_rows {
                truncated = true;
                break;
            }
            let mut cells = Vec::with_capacity(columns.len());
            for column in 0..columns.len() {
                let value = row.as_ref(column).cloned().unwrap_or(Value::NULL);
                cells.push(value_json(value));
            }
            out_rows.push(cells);
        }
        max_warnings = max_warnings.max(qr.warnings());
        let has_grid = !columns.is_empty();
        let affected = if stmt.returns_count {
            if has_grid {
                Some(out_rows.len() as u64)
            } else {
                Some(affected_at_start)
            }
        } else {
            None
        };
        let message = if truncated {
            Some(format!("Result truncated to {max_rows} rows."))
        } else if !has_grid {
            if let Some(rows) = affected {
                Some(affected_message(rows))
            } else {
                Some("Statement executed.".to_string())
            }
        } else {
            None
        };
        let row_count = out_rows.len() as u64;
        let result = QueryResult {
            engine: "mysql".into(),
            mode: batch.mode,
            columns,
            row_count,
            rows: out_rows,
            truncated,
            duration_ms: started.elapsed().as_millis() as u64,
            message,
        };
        sets.push(StatementResult::success(
            stmt,
            set_index,
            result,
            affected,
            Vec::new(),
        ));
        if truncated {
            return Ok((sets, true));
        }
        if qr.is_empty() {
            break;
        }
        set_index = set_index.saturating_add(1);
        if set_index >= 32 {
            return Ok((sets, true));
        }
    }
    if max_warnings > 0 {
        let notes = show_warnings(conn).await;
        if let Some(last) = sets.last_mut() {
            last.notices = if notes.is_empty() {
                vec![if max_warnings == 1 {
                    "1 warning.".to_string()
                } else {
                    format!("{max_warnings} warnings.")
                }]
            } else {
                notes
            };
        }
    }
    Ok((sets, false))
}

async fn show_warnings(conn: &mut mysql_async::Conn) -> Vec<String> {
    let mut qr = match conn.query_iter("SHOW WARNINGS").await {
        Ok(qr) => qr,
        Err(_) => return Vec::new(),
    };
    let width = qr.columns_ref().len();
    let mut notes = Vec::new();
    loop {
        match qr.next().await {
            Ok(Some(row)) => {
                if width >= 3 {
                    if let Some(Value::Bytes(bytes)) = row.as_ref(2).cloned() {
                        let text = String::from_utf8_lossy(&bytes).trim().to_string();
                        if !text.is_empty() {
                            notes.push(text);
                        }
                    }
                }
            }
            _ => break,
        }
    }
    notes
}
