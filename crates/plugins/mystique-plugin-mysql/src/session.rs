use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use mysql_async::consts::ColumnType;
use mysql_async::prelude::*;
use mysql_async::{Pool, Value};
use mystique_connector_api::{
    compile_delete, compile_insert, compile_update, qualify_table, quote_ident,
    split_sql_statements, ApplyTableEditsInput, ApplyTableEditsResult, CatalogColumnRow,
    CatalogDatabase, CatalogObject, CatalogRelationship, ColumnInfo, ConnectorError,
    ConnectorResult, ConnectorSession, EngineCapabilities, FetchTablePreviewInput, PoolStatus,
    QueryColumn, QueryMode, QueryResult, SqlIdentStyle, TablePreviewResult, ValidatedQuery,
    sql_where_clause,
};
use serde_json::{json, Value as JsonValue};

/// SQL-ish type name for query result headers (not Debug enum tags).
fn mysql_type_label(t: ColumnType) -> &'static str {
    use ColumnType::*;
    match t {
        MYSQL_TYPE_DECIMAL | MYSQL_TYPE_NEWDECIMAL => "decimal",
        MYSQL_TYPE_TINY => "tinyint",
        MYSQL_TYPE_SHORT => "smallint",
        MYSQL_TYPE_LONG => "int",
        MYSQL_TYPE_FLOAT => "float",
        MYSQL_TYPE_DOUBLE => "double",
        MYSQL_TYPE_NULL => "null",
        MYSQL_TYPE_TIMESTAMP | MYSQL_TYPE_TIMESTAMP2 => "timestamp",
        MYSQL_TYPE_LONGLONG => "bigint",
        MYSQL_TYPE_INT24 => "mediumint",
        MYSQL_TYPE_DATE | MYSQL_TYPE_NEWDATE => "date",
        MYSQL_TYPE_TIME | MYSQL_TYPE_TIME2 => "time",
        MYSQL_TYPE_DATETIME | MYSQL_TYPE_DATETIME2 => "datetime",
        MYSQL_TYPE_YEAR => "year",
        MYSQL_TYPE_VARCHAR => "varchar",
        MYSQL_TYPE_BIT => "bit",
        MYSQL_TYPE_JSON => "json",
        MYSQL_TYPE_ENUM => "enum",
        MYSQL_TYPE_SET => "set",
        MYSQL_TYPE_TINY_BLOB => "tinyblob",
        MYSQL_TYPE_MEDIUM_BLOB => "mediumblob",
        MYSQL_TYPE_LONG_BLOB => "longblob",
        MYSQL_TYPE_BLOB => "blob",
        MYSQL_TYPE_VAR_STRING => "varchar",
        MYSQL_TYPE_STRING => "char",
        MYSQL_TYPE_GEOMETRY => "geometry",
        MYSQL_TYPE_VECTOR => "vector",
        MYSQL_TYPE_TYPED_ARRAY => "array",
        MYSQL_TYPE_UNKNOWN => "unknown",
    }
}

pub struct MysqlSession {
    pool: Pool,
    max_size: u32,
    closed: AtomicBool,
}

impl MysqlSession {
    pub fn new(pool: Pool, max_size: u32) -> Self {
        Self {
            pool,
            max_size,
            closed: AtomicBool::new(false),
        }
    }
}

fn value_json(v: Value) -> JsonValue {
    match v {
        Value::NULL => JsonValue::Null,
        Value::Bytes(b) => match String::from_utf8(b.clone()) {
            Ok(s) => JsonValue::String(s),
            Err(_) => JsonValue::String(format!(
                "\\x{}",
                b.iter().map(|x| format!("{x:02x}")).collect::<String>()
            )),
        },
        Value::Int(i) => json!(i),
        Value::UInt(u) => json!(u),
        Value::Float(f) => json!(f),
        Value::Double(d) => json!(d),
        // DATE / DATETIME / TIMESTAMP all arrive as Value::Date
        Value::Date(year, month, day, hour, minute, second, micros) => {
            // MySQL zero-date / invalid components
            if year == 0 || month == 0 || day == 0 {
                return JsonValue::Null;
            }
            // Pure DATE (no time component)
            if hour == 0 && minute == 0 && second == 0 && micros == 0 {
                // Could still be midnight DATETIME — still valid ISO form.
                // Prefer full datetime when any time field is non-zero; for
                // pure DATE columns this matches YYYY-MM-DD which is correct.
                return JsonValue::String(format!("{year:04}-{month:02}-{day:02}"));
            }
            if micros == 0 {
                JsonValue::String(format!(
                    "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"
                ))
            } else {
                // Trim trailing zeros in fractional seconds for readability
                let frac = format!("{micros:06}");
                let frac = frac.trim_end_matches('0');
                JsonValue::String(format!(
                    "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{frac}"
                ))
            }
        }
        Value::Time(is_neg, days, hours, minutes, seconds, micros) => {
            let sign = if is_neg { "-" } else { "" };
            if micros == 0 {
                JsonValue::String(format!("{sign}{days} {hours:02}:{minutes:02}:{seconds:02}"))
            } else {
                let frac = format!("{micros:06}");
                let frac = frac.trim_end_matches('0');
                JsonValue::String(format!(
                    "{sign}{days} {hours:02}:{minutes:02}:{seconds:02}.{frac}"
                ))
            }
        }
    }
}

#[async_trait]
impl ConnectorSession for MysqlSession {
    fn engine_id(&self) -> &str {
        "mysql"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::mysql_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;
        conn.query_drop("SELECT 1")
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        Ok(())
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        // mysql_async does not expose idle/active counts cheaply; report configured max.
        Some(PoolStatus {
            engine: "mysql".into(),
            max_size: self.max_size,
            active: 0,
            idle: 0,
            size: self.max_size,
        })
    }

    async fn close(&self) -> ConnectorResult<()> {
        if self
            .closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            // Dropping remaining connections is best-effort; disconnect drains the pool.
            let pool = self.pool.clone();
            let _ = pool.disconnect().await;
        }
        Ok(())
    }

    async fn execute_query(&self, query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }

        // Multi-statement batches: run each in order; return only the last result.
        let start = Instant::now();
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;

        let max_rows = query.max_rows as usize;
        let mode = query.mode;

        let timeout = query.timeout_ms.max(1);
        let _ = conn
            .query_drop(format!("SET SESSION max_execution_time = {timeout}"))
            .await;

        let stmts = split_sql_statements(&query.text);
        if stmts.is_empty() {
            return Err(ConnectorError::Query("Query is empty.".into()));
        }

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
                let result_set = conn
                    .query_iter(sql.as_str())
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;

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
                while let Some(row) = result_set
                    .next()
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?
                {
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
                while result_set
                    .next()
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?
                    .is_some()
                {
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
                conn.query_drop(sql.as_str())
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;

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
        }

        let mut out = last.ok_or_else(|| ConnectorError::Query("No statements to run.".into()))?;
        out.duration_ms = start.elapsed().as_millis() as u64;
        Ok(out)
    }

    async fn describe_table(
        &self,
        database: Option<&str>,
        schema: Option<&str>,
        table: &str,
    ) -> ConnectorResult<Vec<ColumnInfo>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let db = database
            .or(schema)
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "MySQL describe_table requires database/schema".into(),
                )
            })?
            .replace('\'', "''");
        let tbl = table.replace('\'', "''");
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;

        // column_type is already DDL-style (varchar(255), datetime, timestamp, …)
        let sql = format!(
            "SELECT column_name, column_type, is_nullable, column_key \
             FROM information_schema.columns \
             WHERE table_schema = '{db}' AND table_name = '{tbl}' \
             ORDER BY ordinal_position"
        );
        let rows: Vec<(String, String, String, String)> = conn
            .query(sql)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;

        let mut out = Vec::with_capacity(rows.len());
        for (name, column_type, is_nullable, column_key) in rows {
            let enum_values = parse_mysql_enum_values(&column_type);
            out.push(ColumnInfo {
                name,
                data_type: column_type,
                nullable: is_nullable.to_ascii_uppercase() != "NO",
                pk: column_key.to_ascii_uppercase() == "PRI",
                enum_values,
            });
        }
        Ok(out)
    }

    async fn list_catalog_columns(
        &self,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogColumnRow>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;

        let sql = if let Some(db) = database.or(schema) {
            let db = db.replace('\'', "''");
            format!(
                "SELECT table_schema, table_name, column_name, column_type, is_nullable, column_key \
                 FROM information_schema.columns \
                 WHERE table_schema = '{db}' \
                 ORDER BY table_schema, table_name, ordinal_position"
            )
        } else {
            "SELECT table_schema, table_name, column_name, column_type, is_nullable, column_key \
             FROM information_schema.columns \
             WHERE table_schema NOT IN ('mysql','information_schema','performance_schema','sys') \
             ORDER BY table_schema, table_name, ordinal_position"
                .to_string()
        };

        let rows: Vec<(String, String, String, String, String, String)> = conn
            .query(sql)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(
                |(table_schema, table_name, column_name, column_type, is_nullable, column_key)| {
                    CatalogColumnRow {
                        database: Some(table_schema.clone()),
                        schema: Some(table_schema),
                        table: table_name,
                        column: column_name,
                        data_type: column_type,
                        nullable: is_nullable.to_ascii_uppercase() != "NO",
                        pk: column_key.to_ascii_uppercase() == "PRI",
                    }
                },
            )
            .collect())
    }

    async fn apply_table_edits(
        &self,
        edits: &ApplyTableEditsInput,
    ) -> ConnectorResult<ApplyTableEditsResult> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;

        let mut updated = 0u32;
        let mut inserted = 0u32;
        let mut deleted = 0u32;
        for u in &edits.updates {
            let sql = compile_update(SqlIdentStyle::Mysql, edits, u)?;
            conn.query_drop(sql.as_str())
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            updated += 1;
        }
        for i in &edits.inserts {
            let sql = compile_insert(SqlIdentStyle::Mysql, edits, i)?;
            conn.query_drop(sql.as_str())
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            inserted += 1;
        }
        for d in &edits.deletes {
            let sql = compile_delete(SqlIdentStyle::Mysql, edits, d)?;
            conn.query_drop(sql.as_str())
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            deleted += 1;
        }
        Ok(ApplyTableEditsResult {
            updated,
            inserted,
            deleted,
        })
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
        let target = qualify_table(
            SqlIdentStyle::Mysql,
            input.database.as_deref(),
            input.schema.as_deref(),
            &input.table,
        );
        let order = {
            let pks: Vec<String> = columns
                .iter()
                .filter(|c| c.pk)
                .map(|c| quote_ident(SqlIdentStyle::Mysql, &c.name))
                .collect();
            if !pks.is_empty() {
                format!(" ORDER BY {}", pks.join(", "))
            } else if let Some(first) = columns.first() {
                format!(
                    " ORDER BY {}",
                    quote_ident(SqlIdentStyle::Mysql, &first.name)
                )
            } else {
                String::new()
            }
        };
        let limit = input.max_rows;
        let select_list = if columns.is_empty() {
            "*".into()
        } else {
            columns
                .iter()
                .map(|c| quote_ident(SqlIdentStyle::Mysql, &c.name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let offset = input.offset;
        let where_sql = sql_where_clause(SqlIdentStyle::Mysql, &input.filters)?;
        let sql = format!(
            "SELECT {select_list} FROM {target}{where_sql}{order} LIMIT {limit} OFFSET {offset}"
        );

        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;
        let total_rows: u64 = conn
            .query_first(&format!("SELECT COUNT(*) FROM {target}{where_sql}"))
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?
            .unwrap_or(0);
        let result_set = conn
            .query_iter(sql.as_str())
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;

        let col_count = result_set.columns_ref().len();
        let mut out_rows: Vec<Vec<JsonValue>> = Vec::new();
        let mut result_set = result_set;
        while let Some(row) = result_set
            .next()
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?
        {
            if out_rows.len() >= input.max_rows as usize {
                break;
            }
            let mut cells = Vec::with_capacity(col_count);
            for j in 0..col_count {
                let v: Value = row.as_ref(j).cloned().unwrap_or(Value::NULL);
                cells.push(value_json(v));
            }
            out_rows.push(cells);
        }

        let row_count = out_rows.len() as u64;
        let truncated = input.offset as u64 + row_count < total_rows;
        Ok(TablePreviewResult {
            engine: "mysql".into(),
            columns,
            rows: out_rows,
            row_count,
            total_rows,
            truncated,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn list_catalog(
        &self,
        preferred_database: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogDatabase>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;
        let sql =
            "SELECT table_schema, table_name, table_type, COALESCE(table_rows, 0) AS table_rows \
             FROM information_schema.tables \
             WHERE table_schema NOT IN ('mysql','information_schema','performance_schema','sys') \
             ORDER BY table_schema, table_name";
        let rows: Vec<(String, String, String, i64)> = conn
            .query(sql)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;

        let mut by_db: std::collections::BTreeMap<String, Vec<CatalogObject>> =
            std::collections::BTreeMap::new();
        // Include empty databases (no tables yet) so the picker can switch to them.
        if let Ok(schemas) = conn
            .query::<String, _>(
                "SELECT schema_name FROM information_schema.schemata \
                 WHERE schema_name NOT IN ('mysql','information_schema','performance_schema','sys') \
                 ORDER BY schema_name",
            )
            .await
        {
            for name in schemas {
                by_db.entry(name).or_default();
            }
        }
        for (schema, name, raw_type, approx) in rows {
            let kind = if raw_type.to_ascii_lowercase().contains("view") {
                "view"
            } else {
                "table"
            };
            by_db
                .entry(schema.clone())
                .or_default()
                .push(CatalogObject {
                    name,
                    schema: Some(schema.clone()),
                    database: Some(schema),
                    kind: kind.into(),
                    row_count: approx.max(0) as u64,
                });
        }
        if let Some(pref) = preferred_database.map(str::trim).filter(|s| !s.is_empty()) {
            by_db.entry(pref.to_string()).or_default();
        }
        let pref = preferred_database.map(str::trim).unwrap_or("");
        let mut names: Vec<String> = by_db.keys().cloned().collect();
        names.sort_by(|a, b| {
            if !pref.is_empty() && a == pref {
                return std::cmp::Ordering::Less;
            }
            if !pref.is_empty() && b == pref {
                return std::cmp::Ordering::Greater;
            }
            if a == "mystique" {
                return std::cmp::Ordering::Less;
            }
            if b == "mystique" {
                return std::cmp::Ordering::Greater;
            }
            a.cmp(b)
        });
        Ok(names
            .into_iter()
            .map(|name| {
                let objects = by_db.remove(&name).unwrap_or_default();
                let meta = if objects.is_empty() {
                    Some("empty".into())
                } else {
                    Some(format!("{} tables", objects.len()))
                };
                CatalogDatabase {
                    name,
                    meta,
                    system: false,
                    objects,
                }
            })
            .collect())
    }

    async fn list_relationships(
        &self,
        database: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogRelationship>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;
        let sql = if let Some(db) = database.map(str::trim).filter(|s| !s.is_empty()) {
            let db = db.replace('\'', "''");
            format!(
                "SELECT table_name, column_name, referenced_table_name, referenced_column_name, constraint_name \
                 FROM information_schema.key_column_usage \
                 WHERE table_schema = '{db}' AND referenced_table_name IS NOT NULL \
                 ORDER BY table_name, ordinal_position"
            )
        } else {
            "SELECT table_schema, table_name, column_name, referenced_table_schema, referenced_table_name, referenced_column_name, constraint_name \
             FROM information_schema.key_column_usage \
             WHERE referenced_table_name IS NOT NULL \
               AND table_schema NOT IN ('mysql','information_schema','performance_schema','sys') \
             ORDER BY table_schema, table_name"
                .into()
        };
        if database.map(str::trim).filter(|s| !s.is_empty()).is_some() {
            let rows: Vec<(String, String, String, String, String)> = conn
                .query(sql)
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            let schema = database.unwrap().trim().to_string();
            return Ok(rows
                .into_iter()
                .map(
                    |(from_table, from_column, to_table, to_column, name)| CatalogRelationship {
                        from_schema: Some(schema.clone()),
                        from_table,
                        from_column,
                        to_schema: Some(schema.clone()),
                        to_table,
                        to_column,
                        name: Some(name),
                    },
                )
                .collect());
        }
        let rows: Vec<(
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
        )> = conn
            .query(sql)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(
                |(from_schema, from_table, from_column, to_schema, to_table, to_column, name)| {
                    CatalogRelationship {
                        from_schema: Some(from_schema),
                        from_table,
                        from_column,
                        to_schema,
                        to_table,
                        to_column,
                        name: Some(name),
                    }
                },
            )
            .collect())
    }
}

fn parse_mysql_enum_values(column_type: &str) -> Option<Vec<String>> {
    let lower = column_type.to_ascii_lowercase();
    if !(lower.starts_with("enum(") || lower.starts_with("set(")) {
        return None;
    }
    let mut values = Vec::new();
    let bytes = column_type.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            i += 1;
            let mut s = String::new();
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    s.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                }
                if bytes[i] == b'\'' {
                    i += 1;
                    break;
                }
                s.push(bytes[i] as char);
                i += 1;
            }
            values.push(s);
        } else {
            i += 1;
        }
    }
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

#[cfg(test)]
mod live_tests {
    use crate::connector::MysqlConnector;
    use mystique_connector_api::{
        ConnectionConfig, DatabaseConnector, PoolConfig, QueryMode, ValidatedQuery,
    };
    use serde_json::json;

    #[tokio::test]
    async fn list_tables_and_preview_users() {
        let config = ConnectionConfig {
            engine: "mysql".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 3306,
                "database": "mystique",
                "username": "admin"
            }),
            password: Some("123".into()),
            pool: PoolConfig {
                max_size: 2,
                min_idle: 0,
                acquire_timeout_ms: 5000,
                idle_timeout_secs: 30,
            },
        };
        let connector = MysqlConnector;
        let session = connector.connect(&config).await.expect("connect");
        session.ping().await.expect("ping");

        let tables = session
            .execute_query(&ValidatedQuery {
                text: "SELECT table_schema, table_name, table_type FROM information_schema.tables WHERE table_schema NOT IN ('mysql','information_schema','performance_schema','sys') ORDER BY table_name".into(),
                mode: QueryMode::Read,
                max_rows: 100,
                timeout_ms: 10_000,
                collection: None,
                database: None,
            })
            .await
            .expect("list tables");
        assert!(
            tables.row_count >= 5,
            "expected seed tables, got {}",
            tables.row_count
        );
        let names: Vec<String> = tables
            .rows
            .iter()
            .filter_map(|r| r.get(1).and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect();
        assert!(names.iter().any(|n| n == "users"), "names={names:?}");
        assert!(names.iter().any(|n| n == "orders"), "names={names:?}");

        let preview = session
            .execute_query(&ValidatedQuery {
                text: "SELECT * FROM `mystique`.`users` LIMIT 10".into(),
                mode: QueryMode::Read,
                max_rows: 10,
                timeout_ms: 10_000,
                collection: None,
                database: None,
            })
            .await
            .expect("preview");
        assert_eq!(preview.row_count, 10);
        assert!(!preview.columns.is_empty());
        assert!(preview
            .columns
            .iter()
            .any(|c| c.name == "email" || c.name == "id"));

        session.close().await.expect("close");
    }
}

#[cfg(test)]
mod er_meta_tests {
    use crate::connector::MysqlConnector;
    use mystique_connector_api::{
        ConnectionConfig, DatabaseConnector, PoolConfig, QueryMode, ValidatedQuery,
    };
    use serde_json::json;

    #[tokio::test]
    async fn information_schema_columns_and_fks() {
        let config = ConnectionConfig {
            engine: "mysql".into(),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 3306,
                "database": "mystique",
                "username": "admin"
            }),
            password: Some("123".into()),
            pool: PoolConfig {
                max_size: 2,
                min_idle: 0,
                acquire_timeout_ms: 5000,
                idle_timeout_secs: 30,
            },
        };
        let session = MysqlConnector.connect(&config).await.expect("connect");

        let cols = session
            .execute_query(&ValidatedQuery {
                text: "SELECT table_name, column_name, column_type, is_nullable, column_key FROM information_schema.columns WHERE table_schema = 'mystique' ORDER BY table_name, ordinal_position".into(),
                mode: QueryMode::Read,
                max_rows: 5000,
                timeout_ms: 10_000,
                collection: None,
                database: None,
            })
            .await
            .expect("columns");
        assert!(cols.row_count >= 20, "got {}", cols.row_count);

        let fks = session
            .execute_query(&ValidatedQuery {
                text: "SELECT table_name, column_name, referenced_table_name, referenced_column_name FROM information_schema.key_column_usage WHERE table_schema = 'mystique' AND referenced_table_name IS NOT NULL".into(),
                mode: QueryMode::Read,
                max_rows: 100,
                timeout_ms: 10_000,
                collection: None,
                database: None,
            })
            .await
            .expect("fks");
        assert_eq!(fks.row_count, 4, "seed has 4 FKs");
        session.close().await.ok();
    }
}
