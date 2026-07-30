use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use mystique_connector_api::{
    split_sql_statements, CatalogColumnRow, ColumnInfo, ConnectorError, ConnectorResult,
    ConnectorSession, EngineCapabilities, PoolStatus, QueryColumn, QueryResult, ValidatedQuery,
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

    async fn describe_table(
        &self,
        _database: Option<&str>,
        _schema: Option<&str>,
        table: &str,
    ) -> ConnectorResult<Vec<ColumnInfo>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let table = table.to_string();
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> ConnectorResult<Vec<ColumnInfo>> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            // Quote safely: only allow simple identifiers for pragma
            if !table
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
                && !table.contains('"')
            {
                // allow quoted names via replace
            }
            let safe = table.replace('\'', "''");
            let mut stmt = conn
                .prepare(&format!("PRAGMA table_info('{safe}')"))
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            let mut rows = stmt
                .query([])
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| ConnectorError::Query(e.to_string()))?
            {
                // cid, name, type, notnull, dflt_value, pk
                let name: String = row.get(1).map_err(|e| ConnectorError::Query(e.to_string()))?;
                let ty: String = row
                    .get::<_, Option<String>>(2)
                    .map_err(|e| ConnectorError::Query(e.to_string()))?
                    .unwrap_or_else(|| "ANY".into());
                let notnull: i64 = row.get(3).map_err(|e| ConnectorError::Query(e.to_string()))?;
                let pk: i64 = row.get(5).map_err(|e| ConnectorError::Query(e.to_string()))?;
                out.push(ColumnInfo {
                    name,
                    data_type: if ty.is_empty() {
                        "ANY".into()
                    } else {
                        ty
                    },
                    nullable: notnull == 0,
                    pk: pk != 0,
                    enum_values: None,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| ConnectorError::Internal(e.to_string()))?
    }

    async fn list_catalog_columns(
        &self,
        _database: Option<&str>,
        _schema: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogColumnRow>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> ConnectorResult<Vec<CatalogColumnRow>> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            let mut tables_stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master \
                     WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' \
                     ORDER BY name",
                )
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            let tables: Vec<String> = tables_stmt
                .query_map([], |row| row.get(0))
                .map_err(|e| ConnectorError::Query(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| ConnectorError::Query(e.to_string()))?;

            let mut out = Vec::new();
            for table in tables {
                let safe = table.replace('\'', "''");
                let mut stmt = conn
                    .prepare(&format!("PRAGMA table_info('{safe}')"))
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                let mut rows = stmt
                    .query([])
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                while let Some(row) = rows
                    .next()
                    .map_err(|e| ConnectorError::Query(e.to_string()))?
                {
                    let name: String =
                        row.get(1).map_err(|e| ConnectorError::Query(e.to_string()))?;
                    let ty: String = row
                        .get::<_, Option<String>>(2)
                        .map_err(|e| ConnectorError::Query(e.to_string()))?
                        .unwrap_or_else(|| "ANY".into());
                    let notnull: i64 =
                        row.get(3).map_err(|e| ConnectorError::Query(e.to_string()))?;
                    let pk: i64 = row.get(5).map_err(|e| ConnectorError::Query(e.to_string()))?;
                    out.push(CatalogColumnRow {
                        database: None,
                        schema: Some("main".into()),
                        table: table.clone(),
                        column: name,
                        data_type: if ty.is_empty() { "ANY".into() } else { ty },
                        nullable: notnull == 0,
                        pk: pk != 0,
                    });
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| ConnectorError::Internal(e.to_string()))?
    }
}
