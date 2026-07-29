use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use mystique_connector_api::{
    split_sql_statements, ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities,
    PoolStatus, QueryColumn, QueryResult, ValidatedQuery,
};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::types::ValueRef;
use serde_json::{json, Value as JsonValue};

pub struct SqliteSession {
    pool: Pool<SqliteConnectionManager>,
    max_size: u32,
    closed: AtomicBool,
}

impl SqliteSession {
    pub fn new(pool: Pool<SqliteConnectionManager>, max_size: u32) -> Self {
        Self {
            pool,
            max_size,
            closed: AtomicBool::new(false),
        }
    }
}

fn value_ref_json(v: ValueRef<'_>) -> JsonValue {
    match v {
        ValueRef::Null => JsonValue::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => JsonValue::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => JsonValue::String(format!(
            "\\x{}",
            b.iter().map(|x| format!("{x:02x}")).collect::<String>()
        )),
    }
}

#[async_trait]
impl ConnectorSession for SqliteSession {
    fn engine_id(&self) -> &str {
        "sqlite"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::sqlite_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            conn.query_row("SELECT 1", [], |_| Ok(()))
                .map_err(|e| ConnectorError::Query(e.to_string()))
        })
        .await
        .map_err(|e| ConnectorError::Internal(e.to_string()))?
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        let state = self.pool.state();
        Some(PoolStatus {
            engine: "sqlite".into(),
            max_size: self.max_size,
            active: state.connections.saturating_sub(state.idle_connections),
            idle: state.idle_connections,
            size: state.connections,
        })
    }

    async fn close(&self) -> ConnectorResult<()> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn execute_query(&self, query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        // Multi-statement batches: run each in order; return only the last result.
        let start = Instant::now();
        let pool = self.pool.clone();
        let max_rows = query.max_rows as usize;
        let mode = query.mode;
        let stmts = split_sql_statements(&query.text);
        if stmts.is_empty() {
            return Err(ConnectorError::Query("Query is empty.".into()));
        }

        let result = tokio::task::spawn_blocking(move || -> ConnectorResult<QueryResult> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;

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
                    let mut stmt = conn
                        .prepare(sql)
                        .map_err(|e| ConnectorError::Query(e.to_string()))?;

                    let col_count = stmt.column_count();
                    let columns: Vec<QueryColumn> = (0..col_count)
                        .map(|j| QueryColumn {
                            name: stmt.column_name(j).unwrap_or("?").to_string(),
                            data_type: None,
                        })
                        .collect();

                    let mut rows_iter = stmt
                        .query([])
                        .map_err(|e| ConnectorError::Query(e.to_string()))?;

                    let mut out_rows = Vec::new();
                    let mut truncated = false;
                    while let Some(row) = rows_iter
                        .next()
                        .map_err(|e| ConnectorError::Query(e.to_string()))?
                    {
                        if out_rows.len() >= max_rows {
                            truncated = true;
                            break;
                        }
                        let mut cells = Vec::with_capacity(col_count);
                        for j in 0..col_count {
                            let v = row
                                .get_ref(j)
                                .map_err(|e| ConnectorError::Query(e.to_string()))?;
                            cells.push(value_ref_json(v));
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
                        .map_err(|e| ConnectorError::Query(e.to_string()))?;

                    let mut message = Some(
                        "Statement executed. Tabular rows only for SELECT-like queries.".into(),
                    );
                    if batch_n > 1 && i + 1 == batch_n {
                        message = Some(format!(
                            "Statement executed; last of {batch_n} statements."
                        ));
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
            }

            last.ok_or_else(|| ConnectorError::Query("No statements to run.".into()))
        })
        .await
        .map_err(|e| ConnectorError::Internal(e.to_string()))??;

        Ok(QueryResult {
            duration_ms: start.elapsed().as_millis() as u64,
            ..result
        })
    }
}
