//! Policy-validated SQL execution for a pooled Postgres session.

use std::pin::pin;
use std::time::Instant;

use futures_util::{Stream, StreamExt};
use mystique_connector_api::{
    affected_message, emit_progress, opens_transaction, split_sql_statements_for,
    sql_explain_executes, sql_is_proven_read, BatchPhase, ConnectorError, ConnectorResult,
    PlannedStatement, QueryBatchResult, QueryColumn, QueryMode, QueryResult, StatementResult,
    ValidatedBatch, ValidatedQuery, HARD_MAX_ROWS,
};
use mystique_sql::SqlDialect;
use tokio_postgres::error::SqlState;
use tokio_postgres::types::{Format, IsNull, ToSql, Type};
use tokio_postgres::{Client, SimpleQueryMessage};

use super::encode::cell_json;
use super::PostgresSession;
use super::{query_failed, query_failed_at, shift_query_position};

/// Portal name for the capped fetch. Closed before the next statement.
const ROW_CURSOR: &str = "mystique_row_cap";

pub(super) async fn execute_query(
    session: &PostgresSession,
    query: &ValidatedQuery,
) -> ConnectorResult<QueryResult> {
    session.ensure_open()?;

    // Multi-statement batches: run each in order; return only the last result.
    let start = Instant::now();
    let client = session.client().await?;
    let _guard = session.arm_cancel(&client, &query.control);

    let stmts = split_sql_statements_for(&query.text, SqlDialect::Postgres);
    if stmts.is_empty() {
        return Err(ConnectorError::Query("Query is empty.".into()));
    }

    let timeout = query.timeout_ms; // 0 turns statement_timeout off.
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

pub(super) async fn execute_batch(
    session: &PostgresSession,
    batch: &ValidatedBatch,
) -> ConnectorResult<QueryBatchResult> {
    session.ensure_open()?;
    let client = session.client().await?;
    let _guard = session.arm_cancel(&client, &batch.control);
    let timeout = batch.timeout_ms; // 0 turns statement_timeout off.
    let lock_writes = batch.mode == QueryMode::Read || batch.explain_executes;

    if lock_writes {
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
        let result = run_planned_batch(&client, batch, true, true, false).await;
        end_read_only_txn(client, result).await
    } else {
        execute_held(session, &client, batch).await
    }
}

/// Run on a connection the caller keeps. Does not open or close a transaction.
pub(super) async fn execute_held(
    session: &PostgresSession,
    client: &deadpool_postgres::Object,
    batch: &ValidatedBatch,
) -> ConnectorResult<QueryBatchResult> {
    let _guard = session.arm_cancel(client, &batch.control);
    let timeout = batch.timeout_ms; // 0 turns statement_timeout off.
    let _ = client
        .batch_execute(&format!("SET statement_timeout = {timeout}"))
        .await;
    let result = run_planned_batch(client, batch, false, false, false).await;
    let _ = client.batch_execute("RESET statement_timeout").await;
    result
}

/// `in_txn` is whether a transaction block is already open. A cursor SELECT
/// is wrapped in its own BEGIN … COMMIT only when none is. `auto_begin`
/// (manual commit) opens a transaction before the first write.
pub(super) async fn run_planned_batch(
    client: &Client,
    batch: &ValidatedBatch,
    locked: bool,
    in_txn: bool,
    auto_begin: bool,
) -> ConnectorResult<QueryBatchResult> {
    let total = u32::try_from(batch.statements.len()).unwrap_or(u32::MAX);
    let mut results = Vec::with_capacity(batch.statements.len());
    let mut in_txn = locked || in_txn;
    for stmt in &batch.statements {
        if batch.control.is_cancelled() {
            return Err(ConnectorError::Query("Query cancelled.".into()));
        }
        emit_progress(&batch.progress, stmt.index, total, BatchPhase::Started);
        let begun = if auto_begin && !in_txn && opens_transaction(stmt, SqlDialect::Postgres) {
            let begun = client.batch_execute("BEGIN").await;
            in_txn = begun.is_ok();
            begun
        } else {
            Ok(())
        };
        let outcome = match begun {
            Ok(()) => run_planned(client, batch, stmt, &mut in_txn).await?,
            Err(err) => postgres_statement_error(stmt, query_failed(err), &batch.control)?,
        };
        let failed = outcome.error.is_some();
        if stmt.txn_boundary && !locked && !failed {
            in_txn = match transaction_state(client).await {
                Some(state) => state != TxnState::Idle,
                None => txn_mark(&stmt.sql) == Some(TxnMark::Begin),
            };
        }
        let index = outcome.index;
        results.push(outcome);
        emit_progress(&batch.progress, index, total, BatchPhase::Finished);
        if failed {
            if let Some(progress) = &batch.progress {
                progress.flush();
            }
            return Ok(QueryBatchResult {
                results,
                stopped_at: Some(index),
            });
        }
        if locked {
            if let Err(err) = ensure_still_read_only(client).await {
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

async fn run_planned(
    client: &Client,
    batch: &ValidatedBatch,
    stmt: &PlannedStatement,
    in_txn: &mut bool,
) -> ConnectorResult<StatementResult> {
    let started = Instant::now();
    let limit = row_limit(batch.max_rows);
    if stmt.returns_rows {
        let bound = !stmt.params.is_empty();
        let mut opened = false;
        if stmt.cursor && !bound && !*in_txn {
            if let Err(err) = client.batch_execute("BEGIN").await {
                // The failure is in the wrapper, not in the user's statement.
                return postgres_statement_error(stmt, query_failed(err), &batch.control);
            }
            opened = true;
            *in_txn = true;
        }
        let fetched = if bound {
            fetch_bound(client, &stmt.sql, &stmt.params, limit).await
        } else if stmt.cursor {
            fetch_cursor(client, &stmt.sql, limit).await
        } else {
            fetch_utility(client, &stmt.sql, limit).await
        };
        if opened {
            let end = if fetched.is_ok() {
                "COMMIT"
            } else {
                "ROLLBACK"
            };
            if let Err(err) = client.batch_execute(end).await {
                *in_txn = false;
                return postgres_statement_error(
                    stmt,
                    match fetched {
                        Ok(_) => query_failed(err),
                        Err(fetch_err) => fetch_err,
                    },
                    &batch.control,
                );
            }
            *in_txn = false;
        }
        let fetched = match fetched {
            Ok(fetched) => fetched,
            Err(err) => return postgres_statement_error(stmt, err, &batch.control),
        };
        let affected = if stmt.returns_count {
            Some(fetched.rows.len() as u64)
        } else {
            None
        };
        let message = if fetched.truncated {
            Some(format!("Result truncated to {limit} rows."))
        } else {
            None
        };
        let row_count = fetched.rows.len() as u64;
        let result = QueryResult {
            engine: "postgres".into(),
            mode: batch.mode,
            columns: fetched.columns,
            row_count,
            rows: fetched.rows,
            truncated: fetched.truncated,
            duration_ms: started.elapsed().as_millis() as u64,
            message,
        };
        // Notices are logged by the tokio-postgres connection task and are not
        // exposed on a pooled client.
        return Ok(StatementResult::success(
            stmt,
            0,
            result,
            affected,
            Vec::new(),
        ));
    }

    if !stmt.params.is_empty() {
        let binds = q_binds(&stmt.params);
        let refs: Vec<&(dyn ToSql + Sync)> = binds.iter().map(|bind| bind as _).collect();
        return match client.execute(&stmt.sql, &refs).await {
            Ok(count) => Ok(executed_statement(stmt, batch.mode, Some(count), started)),
            Err(err) => {
                postgres_statement_error(stmt, query_failed_at(&stmt.sql, err), &batch.control)
            }
        };
    }

    if stmt.returns_count {
        let messages = match client.simple_query(&stmt.sql).await {
            Ok(messages) => messages,
            Err(err) => {
                return postgres_statement_error(
                    stmt,
                    query_failed_at(&stmt.sql, err),
                    &batch.control,
                )
            }
        };
        let affected = messages
            .into_iter()
            .rev()
            .find_map(|message| match message {
                SimpleQueryMessage::CommandComplete(count) => Some(count),
                _ => None,
            });
        return Ok(executed_statement(stmt, batch.mode, affected, started));
    }

    if let Err(err) = client.batch_execute(&stmt.sql).await {
        return postgres_statement_error(stmt, query_failed_at(&stmt.sql, err), &batch.control);
    }
    Ok(executed_statement(stmt, batch.mode, None, started))
}

fn executed_statement(
    stmt: &PlannedStatement,
    mode: QueryMode,
    affected: Option<u64>,
    started: Instant,
) -> StatementResult {
    let message = if let Some(rows) = affected {
        Some(affected_message(rows))
    } else {
        Some("Statement executed.".to_string())
    };
    let result = QueryResult {
        engine: "postgres".into(),
        mode,
        columns: Vec::new(),
        rows: Vec::new(),
        row_count: 0,
        truncated: false,
        duration_ms: started.elapsed().as_millis() as u64,
        message,
    };
    StatementResult::success(stmt, 0, result, affected, Vec::new())
}

fn postgres_statement_error(
    stmt: &PlannedStatement,
    err: ConnectorError,
    control: &mystique_connector_api::QueryControl,
) -> ConnectorResult<StatementResult> {
    if control.is_cancelled() {
        Err(err)
    } else {
        Ok(StatementResult::failed(stmt, 0, &err))
    }
}

/// Session advisory locks outlive ROLLBACK. A read that took one (the
/// classifier missed it) would leave it on the pooled connection.
pub(super) async fn end_read_only_txn<T>(
    client: deadpool_postgres::Object,
    result: ConnectorResult<T>,
) -> ConnectorResult<T> {
    let mut ended = client
        .batch_execute("ROLLBACK; SELECT pg_advisory_unlock_all()")
        .await;
    if ended.is_err() {
        // A server without advisory locks fails the unlock, not the ROLLBACK.
        ended = client.batch_execute("ROLLBACK").await;
    }
    match ended {
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
    // A caller bug can pass 0 or a huge cap. The driver ceiling is the hard max.
    let limit = row_limit(query.max_rows);
    let mut last: Option<QueryResult> = None;
    let batch_n = stmts.len();
    // Read mode already opened BEGIN READ ONLY. Track later BEGIN/COMMIT so a
    // cursor is not wrapped in a second transaction, and a following SELECT
    // still has one (DECLARE CURSOR requires it).
    let mut in_txn = locked;

    for (i, sql) in stmts.iter().enumerate() {
        let lower = sql.to_ascii_lowercase();
        let result = if let Some(mark) = txn_mark(sql) {
            client
                .batch_execute(sql.as_str())
                .await
                .map_err(query_failed)?;
            in_txn = mark == TxnMark::Begin;
            executed_message(query.mode, batch_n, i)
        } else if is_select_like(&lower) {
            // Same rule as the planner: only a proven read is a cursor. A
            // data-modifying WITH cannot be declared.
            let cursor = is_cursor_source(&lower) && sql_is_proven_read(sql, SqlDialect::Postgres);
            let mut opened = false;
            if cursor && !in_txn {
                client.batch_execute("BEGIN").await.map_err(query_failed)?;
                opened = true;
                in_txn = true;
            }
            let fetched = if cursor {
                fetch_cursor(client, sql, limit).await
            } else {
                // EXPLAIN / SHOW are utility statements. DECLARE would fail and
                // abort the read-only transaction. Their result sets are small.
                fetch_utility(client, sql, limit).await
            };
            if opened {
                let end = if fetched.is_ok() {
                    "COMMIT"
                } else {
                    "ROLLBACK"
                };
                if let Err(err) = client.batch_execute(end).await {
                    return Err(match fetched {
                        Ok(_) => query_failed(err),
                        Err(fetch_err) => fetch_err,
                    });
                }
                in_txn = false;
            }
            let fetched = fetched?;
            let mut message = if fetched.truncated {
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
                columns: fetched.columns,
                row_count: fetched.rows.len() as u64,
                rows: fetched.rows,
                truncated: fetched.truncated,
                duration_ms: 0,
                message,
            }
        } else {
            client
                .batch_execute(sql.as_str())
                .await
                .map_err(query_failed)?;
            executed_message(query.mode, batch_n, i)
        };

        last = Some(result);
        // COMMIT inside the batch would otherwise let the next statement write.
        if locked {
            ensure_still_read_only(client).await?;
        }
    }

    last.ok_or_else(|| ConnectorError::Query("No statements to run.".into()))
}

fn executed_message(mode: QueryMode, batch_n: usize, index: usize) -> QueryResult {
    let mut message = Some(format!(
        "Statement executed ({}). Tabular rows only for SELECT-like queries.",
        match mode {
            QueryMode::Read => "read",
            QueryMode::Write => "write",
        }
    ));
    if batch_n > 1 && index + 1 == batch_n {
        message = Some(format!(
            "{}; last of {batch_n} statements.",
            message.unwrap_or_default().trim_end_matches('.')
        ));
    }
    QueryResult {
        engine: "postgres".into(),
        mode,
        columns: Vec::new(),
        row_count: 0,
        rows: Vec::new(),
        truncated: false,
        duration_ms: 0,
        message,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TxnMark {
    Begin,
    End,
}

/// `BEGIN` / `COMMIT` / `ROLLBACK`, but not `ROLLBACK TO` (the transaction stays open).
fn txn_mark(sql: &str) -> Option<TxnMark> {
    let mut words = sql.split_whitespace();
    let first = words
        .next()
        .unwrap_or("")
        .trim_end_matches(';')
        .to_ascii_lowercase();
    let second = words
        .next()
        .unwrap_or("")
        .trim_end_matches(';')
        .to_ascii_lowercase();
    match first.as_str() {
        "begin" | "start" => Some(TxnMark::Begin),
        "commit" | "end" => Some(TxnMark::End),
        "rollback" | "abort" if second != "to" => Some(TxnMark::End),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TxnState {
    Idle,
    Open,
    /// A statement failed. Only ROLLBACK is accepted; COMMIT also rolls back.
    Failed,
}

/// Whether the session is inside a transaction block.
///
/// tokio-postgres drops the ReadyForQuery status byte. Outside a block the
/// probe runs in its own implicit transaction, so its start time equals the
/// statement's (one simple-protocol message). After BEGIN the transaction
/// started at BEGIN, at least one round trip earlier. An aborted block
/// refuses the probe. `None` when the probe could not run.
pub(super) async fn transaction_state(client: &Client) -> Option<TxnState> {
    let probe = "SELECT transaction_timestamp() <> statement_timestamp()";
    match client.simple_query(probe).await {
        Ok(messages) => messages.iter().find_map(|message| match message {
            SimpleQueryMessage::Row(row) => match row.get(0) {
                Some("t") => Some(TxnState::Open),
                Some(_) => Some(TxnState::Idle),
                None => None,
            },
            _ => None,
        }),
        Err(err) if err.code() == Some(&SqlState::IN_FAILED_SQL_TRANSACTION) => {
            Some(TxnState::Failed)
        }
        Err(_) => None,
    }
}

fn is_select_like(lower: &str) -> bool {
    lower.starts_with("select")
        || lower.starts_with("with")
        || lower.starts_with("values")
        || lower.starts_with("table")
        || lower.starts_with("show")
        || lower.starts_with("explain")
}

/// Statements that can be a server cursor. Utility statements cannot.
fn is_cursor_source(lower: &str) -> bool {
    lower.starts_with("select")
        || lower.starts_with("with")
        || lower.starts_with("values")
        || lower.starts_with("table")
}

fn row_limit(max_rows: u32) -> usize {
    max_rows.clamp(1, HARD_MAX_ROWS) as usize
}

fn fetch_count(limit: usize) -> ConnectorResult<i32> {
    i32::try_from(limit.saturating_add(1))
        .map_err(|_| ConnectorError::Query("Row limit does not fit in a FETCH count.".into()))
}

/// Strip trailing semicolons so `DECLARE CURSOR FOR …` stays one statement.
fn cursor_query(sql: &str) -> &str {
    let mut body = sql.trim();
    while let Some(rest) = body.strip_suffix(';') {
        body = rest.trim_end();
    }
    body
}

struct Fetched {
    columns: Vec<QueryColumn>,
    rows: Vec<Vec<serde_json::Value>>,
    truncated: bool,
}

/// `FETCH limit+1`, then `CLOSE`. The server suspends the portal after that
/// many rows, so a billion-row series is not generated or buffered.
async fn fetch_cursor(client: &Client, sql: &str, limit: usize) -> ConnectorResult<Fetched> {
    let body = cursor_query(sql);
    if body.is_empty() {
        return Err(ConnectorError::Query("Query is empty.".into()));
    }
    let count = fetch_count(limit)?;
    let declare = format!("DECLARE {ROW_CURSOR} NO SCROLL CURSOR FOR {body}");
    let prefix = u32::try_from(declare.len().saturating_sub(body.len())).unwrap_or(0);
    if let Err(err) = client.batch_execute(&declare).await {
        return Err(shift_query_position(query_failed_at(&declare, err), prefix));
    }

    let fetched = read_fetch(client, limit, count).await;
    let closed = client
        .batch_execute(&format!("CLOSE {ROW_CURSOR}"))
        .await
        .map_err(query_failed);
    match (fetched, closed) {
        (Ok(rows), Ok(())) => Ok(rows),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
    }
}

async fn read_fetch(client: &Client, limit: usize, count: i32) -> ConnectorResult<Fetched> {
    let sql = format!("FETCH {count} FROM {ROW_CURSOR}");
    read_stream(client, &sql, limit, false).await
}

/// Parameterized reads skip the simple-protocol cursor. `$1` is not valid there.
async fn fetch_bound(
    client: &Client,
    sql: &str,
    params: &[serde_json::Value],
    limit: usize,
) -> ConnectorResult<Fetched> {
    let binds = q_binds(params);
    let refs: Vec<&(dyn ToSql + Sync)> = binds.iter().map(|bind| bind as _).collect();
    let stream = client
        .query_raw(sql, refs)
        .await
        .map_err(|err| query_failed_at(sql, err))?;
    let mut stream = pin!(stream);
    let capped = collect_capped(&mut stream, limit).await;
    if !matches!(capped, Ok(ref got) if got.exhausted) {
        if let Err(err) = finish_stream(&mut stream).await {
            return Err(query_failed_at(
                sql,
                match capped {
                    Err(fetch_err) => fetch_err,
                    Ok(_) => err,
                },
            ));
        }
    }
    capped
        .map(rows_from)
        .map_err(|err| query_failed_at(sql, err))
}

#[derive(Debug)]
struct QBind(serde_json::Value);

fn q_binds(params: &[serde_json::Value]) -> Vec<QBind> {
    params.iter().cloned().map(QBind).collect()
}

/// Sent in text format. The server parses it with the input function of the
/// type it inferred for `$n` (int4, uuid, timestamptz, numeric, an enum, a
/// domain), the same as a quoted literal. Binary would need an encoder per type.
impl ToSql for QBind {
    fn to_sql(
        &self,
        _ty: &Type,
        out: &mut bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match &self.0 {
            serde_json::Value::Null => return Ok(IsNull::Yes),
            serde_json::Value::String(text) => out.extend_from_slice(text.as_bytes()),
            serde_json::Value::Bool(flag) => {
                out.extend_from_slice(if *flag { b"true" } else { b"false" })
            }
            other => out.extend_from_slice(other.to_string().as_bytes()),
        }
        Ok(IsNull::No)
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }

    fn encode_format(&self, _ty: &Type) -> Format {
        Format::Text
    }

    tokio_postgres::types::to_sql_checked!();
}

/// Small utility result. Not a cursor, so this is not how large SELECTs run.
async fn fetch_utility(client: &Client, sql: &str, limit: usize) -> ConnectorResult<Fetched> {
    read_stream(client, sql, limit, true).await
}

async fn read_stream(
    client: &Client,
    sql: &str,
    limit: usize,
    locate: bool,
) -> ConnectorResult<Fetched> {
    let locate_err = |err: tokio_postgres::Error| {
        if locate {
            query_failed_at(sql, err)
        } else {
            query_failed(err)
        }
    };
    let stream = client
        .query_raw(sql, std::iter::empty::<i32>())
        .await
        .map_err(locate_err)?;
    let mut stream = pin!(stream);
    let capped = collect_capped(&mut stream, limit).await;
    // A truncated FETCH still owes ReadyForQuery. An error does too. Polling
    // again after a clean end asks a closed response channel and fails.
    if !matches!(capped, Ok(ref got) if got.exhausted) {
        if let Err(err) = finish_stream(&mut stream).await {
            return Err(locate_err(match capped {
                Err(fetch_err) => fetch_err,
                Ok(_) => err,
            }));
        }
    }
    capped.map(rows_from).map_err(locate_err)
}

struct Capped<T> {
    kept: Vec<T>,
    truncated: bool,
    /// The stream returned `None` before the extra probe row.
    exhausted: bool,
}

/// Keeps at most `limit` items and pulls at most `limit + 1`.
///
/// The extra item is the truncation probe. It is not stored. Callers that
/// still owe the server a ReadyForQuery must `finish_stream` when
/// `exhausted` is false. Memory of `kept` is `limit`, not the source length.
async fn collect_capped<S, T, E>(mut stream: S, limit: usize) -> Result<Capped<T>, E>
where
    S: Stream<Item = Result<T, E>> + Unpin,
{
    let mut kept = Vec::new();
    let mut seen = 0usize;
    while let Some(item) = stream.next().await {
        let item = item?;
        if kept.len() < limit {
            kept.push(item);
        }
        seen += 1;
        if seen > limit {
            return Ok(Capped {
                kept,
                truncated: true,
                exhausted: false,
            });
        }
    }
    Ok(Capped {
        kept,
        truncated: false,
        exhausted: true,
    })
}

async fn finish_stream<S, T, E>(mut stream: S) -> Result<(), E>
where
    S: Stream<Item = Result<T, E>> + Unpin,
{
    while let Some(item) = stream.next().await {
        let _discard = item?;
    }
    Ok(())
}

fn rows_from(capped: Capped<tokio_postgres::Row>) -> Fetched {
    let mut columns = Vec::new();
    let mut rows = Vec::with_capacity(capped.kept.len());
    for row in &capped.kept {
        if columns.is_empty() {
            columns = row
                .columns()
                .iter()
                .map(|c| QueryColumn {
                    name: c.name().to_string(),
                    data_type: Some(c.type_().name().to_string()),
                })
                .collect();
        }
        let mut cells = Vec::with_capacity(row.len());
        for idx in 0..row.len() {
            cells.push(cell_json(row, idx));
        }
        rows.push(cells);
    }
    Fetched {
        columns,
        rows,
        truncated: capped.truncated,
    }
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

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use super::*;

    struct Pulls<I> {
        iter: I,
        pulls: usize,
    }

    impl<I> Stream for Pulls<I>
    where
        I: Iterator<Item = Result<u32, &'static str>> + Unpin,
    {
        type Item = Result<u32, &'static str>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let this = self.get_mut();
            this.pulls += 1;
            Poll::Ready(this.iter.next())
        }
    }

    #[tokio::test]
    async fn capped_fetch_stops_after_limit_plus_one() {
        // Kept rows are O(limit), not O(series). The 11th value is only a probe.
        // Postgres is asked for that many rows (`FETCH`) and the cursor is closed,
        // so the server does not keep producing the rest of the series.
        let mut stream = Pulls {
            iter: (0u32..).map(Ok),
            pulls: 0,
        };
        let capped = collect_capped(&mut stream, 10).await.expect("cap");
        assert_eq!(capped.kept, (0..10).collect::<Vec<_>>());
        assert!(capped.truncated);
        assert!(!capped.exhausted);
        assert_eq!(stream.pulls, 11, "pulled the probe row and then stopped");
    }

    #[tokio::test]
    async fn short_result_is_not_truncated() {
        let mut stream = Pulls {
            iter: (0u32..3).map(Ok),
            pulls: 0,
        };
        let capped = collect_capped(&mut stream, 10).await.expect("cap");
        assert_eq!(capped.kept.len(), 3);
        assert!(!capped.truncated);
        assert!(capped.exhausted);
    }

    #[test]
    fn row_limit_clamps_zero_and_huge_to_the_driver_ceiling() {
        assert_eq!(row_limit(0), 1);
        assert_eq!(row_limit(10), 10);
        assert_eq!(row_limit(u32::MAX), HARD_MAX_ROWS as usize);
    }

    #[test]
    fn explain_is_not_declared_as_a_cursor() {
        assert!(is_cursor_source("select 1"));
        assert!(is_cursor_source("with s as (select 1) select * from s"));
        assert!(!is_cursor_source("explain select 1"));
        assert!(!is_cursor_source("show statement_timeout"));
    }

    #[test]
    fn cursor_body_drops_trailing_semicolons() {
        assert_eq!(cursor_query(" SELECT 1 ; "), "SELECT 1");
        assert_eq!(cursor_query("SELECT 1;;"), "SELECT 1");
    }

    #[test]
    fn rollback_to_savepoint_stays_in_the_transaction() {
        assert_eq!(txn_mark("BEGIN"), Some(TxnMark::Begin));
        assert_eq!(txn_mark("COMMIT"), Some(TxnMark::End));
        assert_eq!(txn_mark("ROLLBACK TO mystique_row_cap"), None);
        assert_eq!(txn_mark("ROLLBACK"), Some(TxnMark::End));
    }
}
