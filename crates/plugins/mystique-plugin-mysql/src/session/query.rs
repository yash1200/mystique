//! Policy-validated SQL execution for a pooled MySQL session.

use std::time::Instant;

use mysql_async::prelude::*;
use mysql_async::Value;
use mystique_connector_api::{
    affected_message, emit_progress, finish_progress, idle_result, inactivity_limit,
    opens_transaction, poll_until_idle, split_sql_statements_for, sql_explain_executes, BatchPhase,
    ConnectorError, ConnectorResult, InterruptGuard, PlannedStatement, QueryBatchResult,
    QueryColumn, QueryMode, QueryResult, StatementResult, ValidatedBatch, ValidatedQuery,
};
use mystique_sql::SqlDialect;
use serde_json::Value as JsonValue;

use super::cancel::{apply_timeouts, arm_kill, clear_timeouts};
use super::encode::{mysql_type_label, value_json};
use super::read_only::{self, ReadOnlyVar};
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

    // Read mode and EXPLAIN ANALYZE cannot write even when the classifier
    // misses. A rejected lock fails the query closed; we do not fall through
    // to an unconstrained connection.
    let lock_writes = query.mode == QueryMode::Read
        || stmts
            .iter()
            .any(|sql| sql_explain_executes(sql, SqlDialect::Mysql));

    apply_timeouts(&mut conn, query.timeout_ms).await;

    let lock = lock_writes.then_some(session.read_only);
    if let Some(var) = lock {
        if let Err(err) = read_only::lock(&mut conn, var, &query.control).await {
            let _ = conn.disconnect().await;
            return Err(err);
        }
    }
    let result = run_batch(&mut conn, query, &stmts, lock).await;
    let drop_conn = drop_mysql_conn(query, &result);
    query.control.clear_interrupt();
    let ran = release_conn(conn, result, lock, drop_conn).await?;

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

/// A capped last result still has unread packets. Returning the connection
/// would make the driver drain them. Disconnect instead, on both the plain
/// path and the read-only path.
fn drop_mysql_conn(query: &ValidatedQuery, result: &ConnectorResult<QueryResult>) -> bool {
    // Unread packets (row cap or a cancel that aborted mid-stream) must not go
    // back to the pool. A normal SQL error is consumed and the connection is reusable.
    result.as_ref().map(|row| row.truncated).unwrap_or(false) || query.control.is_cancelled()
}

async fn release_conn<T>(
    mut conn: mysql_async::Conn,
    result: ConnectorResult<T>,
    lock: Option<ReadOnlyVar>,
    drop_conn: bool,
) -> ConnectorResult<T> {
    if drop_conn {
        let _ = conn.disconnect().await;
        return result;
    }
    if let Some(var) = lock {
        return read_only::unlock(conn, var, result).await;
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
    lock: Option<ReadOnlyVar>,
) -> ConnectorResult<QueryResult> {
    let max_rows = query.max_rows as usize;
    let mode = query.mode;
    let batch_n = stmts.len();
    let mut last: Option<QueryResult> = None;

    for (i, sql) in stmts.iter().enumerate() {
        let is_last = i + 1 == batch_n;
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
                        poll_until_idle(
                            &query.control,
                            inactivity_limit(query.timeout_ms),
                            result_set.next(),
                        )
                        .await,
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
                    // Read past the cap so the next statement can run.
                    if is_last {
                        break;
                    }
                    continue;
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

        last = Some(result);
        if let (Some(var), false) = (lock, is_last) {
            read_only::ensure_locked(conn, var).await?;
        }
    }

    last.ok_or_else(|| ConnectorError::Query("No statements to run.".into()))
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

    let lock = lock_writes.then_some(session.read_only);
    if let Some(var) = lock {
        if let Err(err) = read_only::lock(&mut conn, var, &batch.control).await {
            let _ = conn.disconnect().await;
            return Err(err);
        }
    }
    let result = run_planned_batch(&mut conn, batch, lock, false).await;
    let drop_conn = drop_mysql_batch(batch, &result);
    batch.control.clear_interrupt();
    release_conn(conn, result, lock, drop_conn).await
}

/// Only the last statement can leave a capped result unread; earlier ones are
/// read to the end so the batch can go on.
fn drop_mysql_batch(batch: &ValidatedBatch, result: &ConnectorResult<QueryBatchResult>) -> bool {
    batch.control.is_cancelled()
        || result.as_ref().is_ok_and(|ran| {
            ran.results
                .last()
                .and_then(|stmt| stmt.result.as_ref())
                .is_some_and(|row| row.truncated)
        })
}

/// `START TRANSACTION` unless the server says one is already open.
async fn begin_unless_open(conn: &mut mysql_async::Conn) -> ConnectorResult<()> {
    if transaction_open(conn).await? {
        return Ok(());
    }
    conn.query_drop("START TRANSACTION")
        .await
        .map_err(query_failed)
}

/// The server's in-transaction status bit, read from the OK packet of `DO 0`.
/// Unlike `innodb_trx`, it is set right after `BEGIN` and needs no privilege.
pub(super) async fn transaction_open(conn: &mut mysql_async::Conn) -> ConnectorResult<bool> {
    conn.query_drop("DO 0").await.map_err(query_failed)?;
    Ok(conn.last_ok_packet().is_some_and(|ok| {
        ok.status_flags()
            .contains(mysql_async::consts::StatusFlags::SERVER_STATUS_IN_TRANS)
    }))
}

fn located_batch(batch: &ValidatedBatch, sql: &str, err: mysql_async::Error) -> ConnectorError {
    if batch.control.is_cancelled() {
        ConnectorError::Query("Query cancelled.".into())
    } else {
        query_failed_at(sql, err)
    }
}

/// `auto_begin` (manual commit) starts a transaction before a write when none
/// is open. DDL commits implicitly, so the check runs before every write.
pub(super) async fn run_planned_batch(
    conn: &mut mysql_async::Conn,
    batch: &ValidatedBatch,
    lock: Option<ReadOnlyVar>,
    auto_begin: bool,
) -> ConnectorResult<QueryBatchResult> {
    let total = u32::try_from(batch.statements.len()).unwrap_or(u32::MAX);
    let mut results = Vec::with_capacity(batch.statements.len());
    for (position, stmt) in batch.statements.iter().enumerate() {
        let is_last = position + 1 == batch.statements.len();
        if batch.control.is_cancelled() {
            return Err(ConnectorError::Query("Query cancelled.".into()));
        }
        emit_progress(&batch.progress, stmt.index, total, BatchPhase::Started);
        let begun = if auto_begin && opens_transaction(stmt, SqlDialect::Mysql) {
            begin_unless_open(conn).await
        } else {
            Ok(())
        };
        let drained = match begun {
            Ok(()) => drain_sets(conn, batch, stmt, !is_last).await,
            Err(err) => Err(err),
        };
        let (sets, unread) = match drained {
            Ok(drained) => drained,
            Err(err) => {
                if batch.control.is_cancelled() {
                    return Err(err);
                }
                results.push(StatementResult::failed(stmt, 0, &err));
                emit_progress(&batch.progress, stmt.index, total, BatchPhase::Finished);
                finish_progress(&batch.progress);
                return Ok(QueryBatchResult {
                    results,
                    stopped_at: Some(stmt.index),
                });
            }
        };
        let index = stmt.index;
        results.extend(sets);
        emit_progress(&batch.progress, index, total, BatchPhase::Finished);
        // A capped last result still has unread packets; the caller
        // disconnects. Truncation is not a statement error.
        if unread || is_last {
            break;
        }
        if let Some(var) = lock {
            if let Err(err) = read_only::ensure_locked(conn, var).await {
                if let Some(last) = results.last_mut() {
                    last.error = Some(err.to_string());
                }
                finish_progress(&batch.progress);
                return Ok(QueryBatchResult {
                    results,
                    stopped_at: Some(index),
                });
            }
        }
    }
    finish_progress(&batch.progress);
    Ok(QueryBatchResult {
        results,
        stopped_at: None,
    })
}

async fn drain_sets(
    conn: &mut mysql_async::Conn,
    batch: &ValidatedBatch,
    stmt: &PlannedStatement,
    read_all: bool,
) -> ConnectorResult<(Vec<StatementResult>, bool)> {
    let (mut sets, stopped, warnings) = if stmt.params.is_empty() {
        let mut qr = conn
            .query_iter(stmt.sql.as_str())
            .await
            .map_err(|err| located_batch(batch, &stmt.sql, err))?;
        read_mysql_sets(&mut qr, batch, stmt, read_all).await?
    } else {
        let mut qr = conn
            .exec_iter(stmt.sql.as_str(), super::edits::mysql_params(&stmt.params))
            .await
            .map_err(|err| located_batch(batch, &stmt.sql, err))?;
        read_mysql_sets(&mut qr, batch, stmt, read_all).await?
    };
    if warnings > 0 {
        let notes = show_warnings(conn).await;
        if let Some(last) = sets.last_mut() {
            last.notices = if notes.is_empty() {
                vec![if warnings == 1 {
                    "1 warning.".to_string()
                } else {
                    format!("{warnings} warnings.")
                }]
            } else {
                notes
            };
        }
    }
    Ok((sets, stopped))
}

/// The flag is true when packets were left unread. With `read_all`, rows
/// past the cap are read and dropped so the connection can run more. A
/// statement that reports a count (`DELETE … RETURNING`) is always read to
/// the end so the count is every changed row, not the rows shown.
async fn read_mysql_sets<P>(
    qr: &mut mysql_async::QueryResult<'_, '_, P>,
    batch: &ValidatedBatch,
    stmt: &PlannedStatement,
    read_all: bool,
) -> ConnectorResult<(Vec<StatementResult>, bool, u16)>
where
    P: mysql_async::prelude::Protocol,
{
    let max_rows = batch.max_rows.max(1) as usize;
    let read_all = read_all || stmt.returns_count;
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
        let mut seen: u64 = 0;
        loop {
            if batch.control.is_cancelled() {
                return Err(ConnectorError::Query("Query cancelled.".into()));
            }
            let step = if saw_row {
                idle_result(
                    poll_until_idle(
                        &batch.control,
                        inactivity_limit(batch.timeout_ms),
                        qr.next(),
                    )
                    .await,
                )?
            } else {
                qr.next().await
            };
            let Some(row) = step.map_err(|err| located_batch(batch, &stmt.sql, err))? else {
                break;
            };
            saw_row = true;
            seen = seen.saturating_add(1);
            if columns.is_empty() {
                continue;
            }
            if out_rows.len() >= max_rows {
                truncated = true;
                if !read_all {
                    break;
                }
                continue;
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
                Some(seen)
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
        if truncated && !read_all {
            return Ok((sets, true, max_warnings));
        }
        if qr.is_empty() {
            break;
        }
        set_index = set_index.saturating_add(1);
        if set_index >= 32 {
            // With `read_all` the driver drops the remaining sets before the
            // next statement.
            return Ok((sets, !read_all, max_warnings));
        }
    }
    Ok((sets, false, max_warnings))
}

async fn show_warnings(conn: &mut mysql_async::Conn) -> Vec<String> {
    let mut qr = match conn.query_iter("SHOW WARNINGS").await {
        Ok(qr) => qr,
        Err(_) => return Vec::new(),
    };
    let width = qr.columns_ref().len();
    let mut notes = Vec::new();
    while let Ok(Some(row)) = qr.next().await {
        if width >= 3 {
            if let Some(Value::Bytes(bytes)) = row.as_ref(2).cloned() {
                let text = String::from_utf8_lossy(&bytes).trim().to_string();
                if !text.is_empty() {
                    notes.push(text);
                }
            }
        }
    }
    notes
}
