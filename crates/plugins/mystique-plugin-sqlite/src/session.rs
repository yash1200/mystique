use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use mystique_connector_api::{
    compile_delete, compile_insert, compile_update, quote_ident, split_sql_statements,
    ApplyTableEditsInput, ApplyTableEditsResult, CatalogColumnRow, CatalogDatabase, CatalogObject,
    CatalogRelationship,
    ColumnInfo, ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities,
    FetchTablePreviewInput, PoolStatus, QueryColumn, QueryResult, SqlIdentStyle, TablePreviewResult,
    ValidatedQuery,
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
                let name: String = row
                    .get(1)
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                let ty: String = row
                    .get::<_, Option<String>>(2)
                    .map_err(|e| ConnectorError::Query(e.to_string()))?
                    .unwrap_or_else(|| "ANY".into());
                let notnull: i64 = row
                    .get(3)
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                let pk: i64 = row
                    .get(5)
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                out.push(ColumnInfo {
                    name,
                    data_type: if ty.is_empty() { "ANY".into() } else { ty },
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
                    let name: String = row
                        .get(1)
                        .map_err(|e| ConnectorError::Query(e.to_string()))?;
                    let ty: String = row
                        .get::<_, Option<String>>(2)
                        .map_err(|e| ConnectorError::Query(e.to_string()))?
                        .unwrap_or_else(|| "ANY".into());
                    let notnull: i64 = row
                        .get(3)
                        .map_err(|e| ConnectorError::Query(e.to_string()))?;
                    let pk: i64 = row
                        .get(5)
                        .map_err(|e| ConnectorError::Query(e.to_string()))?;
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

    async fn apply_table_edits(
        &self,
        edits: &ApplyTableEditsInput,
    ) -> ConnectorResult<ApplyTableEditsResult> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let mut statements = Vec::new();
        for u in &edits.updates {
            statements.push(compile_update(SqlIdentStyle::Postgres, edits, u)?);
        }
        let update_n = statements.len();
        for i in &edits.inserts {
            statements.push(compile_insert(SqlIdentStyle::Postgres, edits, i)?);
        }
        let insert_n = statements.len() - update_n;
        for d in &edits.deletes {
            statements.push(compile_delete(SqlIdentStyle::Postgres, edits, d)?);
        }
        let delete_n = statements.len() - update_n - insert_n;
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> ConnectorResult<ApplyTableEditsResult> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            for sql in &statements {
                conn.execute(sql.as_str(), [])
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
            }
            Ok(ApplyTableEditsResult {
                updated: update_n as u32,
                inserted: insert_n as u32,
                deleted: delete_n as u32,
            })
        })
        .await
        .map_err(|e| ConnectorError::Internal(e.to_string()))?
    }

    async fn fetch_table_preview(
        &self,
        input: &FetchTablePreviewInput,
    ) -> ConnectorResult<TablePreviewResult> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let start = Instant::now();
        let columns = self
            .describe_table(
                input.database.as_deref(),
                input.schema.as_deref(),
                &input.table,
            )
            .await?;
        let target = quote_ident(SqlIdentStyle::Postgres, &input.table);
        let order = {
            let pks: Vec<String> = columns
                .iter()
                .filter(|c| c.pk)
                .map(|c| quote_ident(SqlIdentStyle::Postgres, &c.name))
                .collect();
            if !pks.is_empty() {
                format!(" ORDER BY {}", pks.join(", "))
            } else {
                " ORDER BY rowid".into()
            }
        };
        let limit = input.max_rows;
        let select_list = if columns.is_empty() {
            "*".into()
        } else {
            columns
                .iter()
                .map(|c| quote_ident(SqlIdentStyle::Postgres, &c.name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let offset = input.offset;
        let sql = format!(
            "SELECT {select_list} FROM {target}{order} LIMIT {limit} OFFSET {offset}"
        );
        let count_sql = format!("SELECT COUNT(*) FROM {target}");
        let pool = self.pool.clone();
        let max_rows = input.max_rows as usize;
        let (result, total_rows) = tokio::task::spawn_blocking(
            move || -> ConnectorResult<(Vec<Vec<JsonValue>>, u64)> {
                let conn = pool
                    .get()
                    .map_err(|e| ConnectorError::Pool(e.to_string()))?;
                let total_rows: i64 = conn
                    .query_row(&count_sql, [], |r| r.get(0))
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                let col_count = stmt.column_count();
                let mut rows_iter = stmt
                    .query([])
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                let mut out_rows = Vec::new();
                while let Some(row) = rows_iter
                    .next()
                    .map_err(|e| ConnectorError::Query(e.to_string()))?
                {
                    if out_rows.len() >= max_rows {
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
                Ok((out_rows, total_rows.max(0) as u64))
            },
        )
        .await
        .map_err(|e| ConnectorError::Internal(e.to_string()))??;

        let row_count = result.len() as u64;
        let truncated = input.offset as u64 + row_count < total_rows;
        Ok(TablePreviewResult {
            engine: "sqlite".into(),
            columns,
            rows: result,
            row_count,
            total_rows,
            truncated,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn list_catalog(
        &self,
        _preferred_database: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogDatabase>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> ConnectorResult<Vec<CatalogDatabase>> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT name, type FROM sqlite_master \
                     WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' \
                     ORDER BY name",
                )
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            let mut rows = stmt
                .query([])
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            let mut objects = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| ConnectorError::Query(e.to_string()))?
            {
                let name: String = row
                    .get(0)
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                let raw_type: String = row
                    .get(1)
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                let kind = if raw_type.to_ascii_lowercase().contains("view") {
                    "view"
                } else {
                    "table"
                };
                objects.push(CatalogObject {
                    name,
                    schema: None,
                    database: Some("main".into()),
                    kind: kind.into(),
                    row_count: 0,
                });
            }
            Ok(vec![CatalogDatabase {
                name: "main".into(),
                meta: Some("live".into()),
                system: false,
                objects,
            }])
        })
        .await
        .map_err(|e| ConnectorError::Internal(e.to_string()))?
    }

    async fn list_relationships(
        &self,
        _database: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogRelationship>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || -> ConnectorResult<Vec<CatalogRelationship>> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            let mut tables_stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master \
                     WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
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
                let mut fk_stmt = conn
                    .prepare(&format!("PRAGMA foreign_key_list('{safe}')"))
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                let mut rows = fk_stmt
                    .query([])
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                while let Some(row) = rows
                    .next()
                    .map_err(|e| ConnectorError::Query(e.to_string()))?
                {
                    // id, seq, table, from, to, on_update, on_delete, match
                    let to_table: String = row
                        .get(2)
                        .map_err(|e| ConnectorError::Query(e.to_string()))?;
                    let from_col: String = row
                        .get(3)
                        .map_err(|e| ConnectorError::Query(e.to_string()))?;
                    let to_col: String = row
                        .get(4)
                        .map_err(|e| ConnectorError::Query(e.to_string()))?;
                    out.push(CatalogRelationship {
                        from_schema: None,
                        from_table: table.clone(),
                        from_column: from_col,
                        to_schema: None,
                        to_table,
                        to_column: to_col,
                        name: None,
                    });
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| ConnectorError::Internal(e.to_string()))?
    }
}
