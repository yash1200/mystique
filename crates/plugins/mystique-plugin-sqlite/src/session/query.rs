//! Policy-validated SQL execution for a pooled SQLite session.

use std::sync::Arc;
use std::time::Instant;

use mystique_connector_api::{
    split_sql_statements_for, sql_explain_executes, ConnectorError, ConnectorResult, QueryColumn,
    QueryMode, QueryResult, SqlDialect, ValidatedQuery,
};
use rusqlite::Connection;

use super::encode::value_ref_json;
use super::interrupt::{self, map_err, stalled};
use super::query_failed;
use super::SqliteSession;

pub(super) async fn execute_query(
    session: &SqliteSession,
    query: &ValidatedQuery,
) -> ConnectorResult<QueryResult> {
    session.ensure_open()?;
    // Multi-statement batches: run each in order; return only the last result.
    let start = Instant::now();
    let pool = session.pool();
    let max_rows = query.max_rows as usize;
    let mode = query.mode;
    let stmts = split_sql_statements_for(&query.text, SqlDialect::Sqlite);
    if stmts.is_empty() {
        return Err(ConnectorError::Query("Query is empty.".into()));
    }

    let lock_writes = mode == QueryMode::Read
        || stmts
            .iter()
            .any(|sql| sql_explain_executes(sql, SqlDialect::Sqlite));
    // `PRAGMA query_only` must not be set on a pooled connection. A later
    // checkout would stay read-only if we forgot to turn it off. A dedicated
    // connection is dropped at the end of this call, which is the only way
    // the pragma is cleared.
    let path = session.path();
    let control = Arc::clone(&query.control);

    let result = session
        .blocking(move || -> ConnectorResult<QueryResult> {
            let pooled;
            let dedicated;
            let conn: &Connection = if lock_writes {
                dedicated = Connection::open(&path).map_err(|err| map_err(&control, err))?;
                dedicated
                    .execute_batch("PRAGMA query_only = ON")
                    .map_err(|err| map_err(&control, err))?;
                &dedicated
            } else {
                pooled = pool
                    .get()
                    .map_err(|e| ConnectorError::Pool(e.to_string()))?;
                &pooled
            };
            interrupt::arm(conn, &control);
            let _settle = interrupt::Settle {
                conn,
                control: &control,
            };

            let batch_n = stmts.len();
            let mut last: Option<QueryResult> = None;

            for (i, sql) in stmts.iter().enumerate() {
                let lower = sql.to_ascii_lowercase();
                let is_select_like = lower.starts_with("select")
                    || lower.starts_with("with")
                    || lower.starts_with("pragma")
                    || lower.starts_with("explain")
                    || lower.starts_with("values");

                let result = if is_select_like {
                    let mut stmt = conn.prepare(sql).map_err(|err| map_err(&control, err))?;

                    let col_count = stmt.column_count();
                    // Declared affinity/type when available (CREATE TABLE).
                    let columns: Vec<QueryColumn> = stmt
                        .columns()
                        .into_iter()
                        .map(|c| QueryColumn {
                            name: c.name().to_string(),
                            data_type: c
                                .decl_type()
                                .map(|s| s.to_string())
                                .filter(|s| !s.is_empty()),
                        })
                        .collect();

                    let mut rows_iter = stmt.query([]).map_err(|err| map_err(&control, err))?;

                    let mut out_rows = Vec::new();
                    let mut truncated = false;
                    let mut saw_row = false;
                    let mut last_row = Instant::now();
                    while let Some(row) = rows_iter.next().map_err(|err| map_err(&control, err))? {
                        stalled(&control, saw_row, last_row)?;
                        if out_rows.len() >= max_rows {
                            truncated = true;
                            break;
                        }
                        let mut cells = Vec::with_capacity(col_count);
                        for j in 0..col_count {
                            let v = row.get_ref(j).map_err(|err| map_err(&control, err))?;
                            cells.push(value_ref_json(v));
                        }
                        out_rows.push(cells);
                        saw_row = true;
                        last_row = Instant::now();
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
                        engine: "sqlite".into(),
                        mode,
                        columns,
                        row_count: out_rows.len() as u64,
                        rows: out_rows,
                        truncated,
                        duration_ms: 0,
                        message,
                    }
                } else {
                    conn.execute(sql, [])
                        .map_err(|err| map_err(&control, err))?;

                    let mut message = Some(
                        "Statement executed. Tabular rows only for SELECT-like queries.".into(),
                    );
                    if batch_n > 1 && i + 1 == batch_n {
                        message =
                            Some(format!("Statement executed; last of {batch_n} statements."));
                    }

                    QueryResult {
                        engine: "sqlite".into(),
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
                if lock_writes {
                    ensure_still_read_only(conn)?;
                }
            }

            last.ok_or_else(|| ConnectorError::Query("No statements to run.".into()))
        })
        .await?;

    Ok(QueryResult {
        duration_ms: start.elapsed().as_millis() as u64,
        ..result
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use mystique_connector_api::{
        ConnectionConfig, DatabaseConnector, PoolConfig, QueryControl, QueryMode, ValidatedQuery,
    };
    use serde_json::json;

    use crate::SqliteConnector;

    fn query(text: &str, mode: QueryMode) -> ValidatedQuery {
        ValidatedQuery {
            text: text.into(),
            mode,
            max_rows: 50,
            timeout_ms: 5_000,
            collection: None,
            database: None,
            control: QueryControl::new(),
        }
    }

    fn temp_path() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("mystique-ro-{}-{nanos}.sqlite", std::process::id()))
    }

    #[tokio::test]
    async fn read_mode_delete_does_not_write_and_pooled_write_still_works() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let session = SqliteConnector
            .connect(&ConnectionConfig {
                engine: "sqlite".into(),
                plugin_config: json!({ "path": path.to_string_lossy() }),
                password: None,
                pool: PoolConfig {
                    max_size: 1,
                    min_idle: 0,
                    acquire_timeout_ms: 3_000,
                    idle_timeout_secs: 5,
                },
            })
            .await
            .expect("connect");

        session
            .execute_query(&query(
                "CREATE TABLE t (id INTEGER); INSERT INTO t VALUES (1)",
                QueryMode::Write,
            ))
            .await
            .expect("seed");

        let err = session
            .execute_query(&query("DELETE FROM t", QueryMode::Read))
            .await
            .expect_err("delete in read mode");
        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("readonly") || msg.contains("read-only"),
            "{msg}"
        );

        let count = session
            .execute_query(&query("SELECT COUNT(*) FROM t", QueryMode::Read))
            .await
            .expect("count");
        assert_eq!(count.rows[0][0], json!(1));

        let drop_err = session
            .execute_query(&query("DROP TABLE t", QueryMode::Read))
            .await
            .expect_err("drop in read mode");
        assert!(
            drop_err
                .to_string()
                .to_ascii_lowercase()
                .contains("readonly")
                || drop_err
                    .to_string()
                    .to_ascii_lowercase()
                    .contains("read-only"),
            "{drop_err}"
        );

        session
            .execute_query(&query("DELETE FROM t", QueryMode::Write))
            .await
            .expect("pooled write after read-only");

        let pragma = session
            .execute_query(&query("PRAGMA query_only", QueryMode::Write))
            .await
            .expect("pragma");
        assert_eq!(pragma.rows[0][0], json!(0));

        let _ = session.close().await;
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn disabling_query_only_does_not_run_the_following_delete() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let session = SqliteConnector
            .connect(&ConnectionConfig {
                engine: "sqlite".into(),
                plugin_config: json!({ "path": path.to_string_lossy() }),
                password: None,
                pool: PoolConfig::default(),
            })
            .await
            .expect("connect");
        session
            .execute_query(&query(
                "CREATE TABLE t (id INTEGER); INSERT INTO t VALUES (1)",
                QueryMode::Write,
            ))
            .await
            .expect("seed");

        let err = session
            .execute_query(&query(
                "PRAGMA query_only = OFF; DELETE FROM t",
                QueryMode::Read,
            ))
            .await
            .expect_err("batch must stop");
        assert!(err.to_string().contains("turned off"), "{}", err);

        let count = session
            .execute_query(&query("SELECT COUNT(*) FROM t", QueryMode::Write))
            .await
            .expect("count");
        assert_eq!(count.rows[0][0], json!(1));

        let _ = session.close().await;
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn recursive_cte_stops_when_the_control_is_cancelled() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let session = SqliteConnector
            .connect(&ConnectionConfig {
                engine: "sqlite".into(),
                plugin_config: json!({ "path": path.to_string_lossy() }),
                password: None,
                pool: PoolConfig::default(),
            })
            .await
            .expect("connect");
        let control = QueryControl::new();
        let watched = std::sync::Arc::clone(&control);
        let query = ValidatedQuery {
            text: "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c LIMIT 50000000) SELECT sum(x) FROM c".into(),
            mode: QueryMode::Read,
            max_rows: 10,
            timeout_ms: 30_000,
            collection: None,
            database: None,
            control,
        };
        let canceller = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            watched.cancel();
        });
        let started = std::time::Instant::now();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            session.execute_query(&query),
        )
        .await;
        let _ = canceller.await;
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "interrupt did not stop the recursive CTE"
        );
        let err = result.expect("query future").expect_err("cancelled");
        let msg = err.to_string().to_ascii_lowercase();
        assert!(msg.contains("cancel") || msg.contains("interrupt"), "{msg}");
        let _ = session.close().await;
        let _ = fs::remove_file(&path);
    }
}

fn ensure_still_read_only(conn: &Connection) -> ConnectorResult<()> {
    let on: i64 = conn
        .query_row("PRAGMA query_only", [], |row| row.get(0))
        .map_err(query_failed)?;
    if on != 0 {
        return Ok(());
    }
    Err(ConnectorError::Query(
        "The read-only connection was turned off before every statement ran. Later statements were not executed.".into(),
    ))
}
