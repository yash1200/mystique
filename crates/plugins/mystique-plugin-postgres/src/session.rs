use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use deadpool_postgres::Pool;
use mystique_connector_api::{
    cell_bool, compile_delete, compile_insert, compile_update, qualify_table,
    quote_ident, split_sql_statements, ApplyTableEditsInput, ApplyTableEditsResult,
    CatalogColumnRow, CatalogDatabase, CatalogObject, CatalogRelationship, ColumnInfo,
    ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities, FetchTablePreviewInput,
    ColumnReference, ObjectStructure, PoolStatus, QueryColumn, QueryMode, QueryResult,
    SqlIdentStyle, StructureSection, TableAdminInput, TableAdminOp, TableAdminResult,
    TableObjectKind, TablePreviewResult, ValidatedQuery, preview_sort_sql, sql_where_clause,
};
use serde_json::{json, Value as JsonValue};
use tokio_postgres::types::Type;
use tokio_postgres::Row;

pub struct PostgresSession {
    pool: Pool,
    max_size: u32,
    closed: AtomicBool,
}

impl PostgresSession {
    pub fn new(pool: Pool, max_size: u32) -> Self {
        Self {
            pool,
            max_size,
            closed: AtomicBool::new(false),
        }
    }
}

fn opt_string(s: Option<String>) -> JsonValue {
    s.map(JsonValue::String).unwrap_or(JsonValue::Null)
}

/// Map information_schema prose → PostgreSQL udt-style developer labels.
fn pg_info_schema_to_udt(data_type: &str) -> String {
    match data_type.to_ascii_lowercase().as_str() {
        "timestamp without time zone" => "timestamp".into(),
        "timestamp with time zone" => "timestamptz".into(),
        "time without time zone" => "time".into(),
        "time with time zone" => "timetz".into(),
        "character varying" => "varchar".into(),
        "character" => "char".into(),
        "double precision" => "float8".into(),
        "bit varying" => "varbit".into(),
        "boolean" => "bool".into(),
        "integer" => "int4".into(),
        "smallint" => "int2".into(),
        "bigint" => "int8".into(),
        "real" => "float4".into(),
        "decimal" => "numeric".into(),
        other => other.to_string(),
    }
}

/// Developer-facing PG type: `timestamptz`, `varchar(255)`, `numeric(12,2)`, `int4[]`.
fn pg_type_label(
    udt_name: &str,
    data_type: &str,
    char_max: Option<i32>,
    num_prec: Option<i32>,
    num_scale: Option<i32>,
    dt_prec: Option<i32>,
) -> String {
    let mut udt = udt_name.trim().to_string();
    if udt.is_empty() {
        udt = pg_info_schema_to_udt(data_type);
    }
    if udt.is_empty() {
        return "unknown".into();
    }
    // Arrays: `_int4` → `int4[]`
    if let Some(elem) = udt.strip_prefix('_') {
        return format!("{elem}[]");
    }
    if matches!(udt.as_str(), "varchar" | "bpchar" | "bit" | "varbit") {
        if let Some(n) = char_max {
            let base = if udt == "bpchar" {
                "char"
            } else {
                udt.as_str()
            };
            return format!("{base}({n})");
        }
        if udt == "bpchar" {
            return "char".into();
        }
    }
    if udt == "numeric" || udt == "decimal" {
        return match (num_prec, num_scale) {
            (Some(p), Some(s)) => format!("numeric({p},{s})"),
            (Some(p), None) => format!("numeric({p})"),
            _ => "numeric".into(),
        };
    }
    // Default fractional precision is 6 — omit unless non-default.
    if matches!(
        udt.as_str(),
        "timestamp" | "timestamptz" | "time" | "timetz"
    ) {
        if let Some(p) = dt_prec {
            if p != 6 {
                return format!("{udt}({p})");
            }
        }
    }
    udt
}

fn pg_opt_i32(v: Option<i32>) -> Option<i32> {
    v
}

/// Format naive timestamp without trailing empty fractional part.
fn fmt_naive_dt(dt: NaiveDateTime) -> String {
    let s = dt.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn fmt_naive_time(t: NaiveTime) -> String {
    let s = t.format("%H:%M:%S%.6f").to_string();
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn cell_json(row: &Row, idx: usize) -> JsonValue {
    let ty = row.columns()[idx].type_();
    match *ty {
        Type::BOOL => match row.try_get::<_, Option<bool>>(idx) {
            Ok(v) => v.map(JsonValue::Bool).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::INT2 | Type::INT4 => match row.try_get::<_, Option<i32>>(idx) {
            Ok(v) => v.map(|n| json!(n)).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::INT8 => match row.try_get::<_, Option<i64>>(idx) {
            Ok(v) => v.map(|n| json!(n)).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::FLOAT4 | Type::FLOAT8 => match row.try_get::<_, Option<f64>>(idx) {
            Ok(v) => v.map(|n| json!(n)).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::JSON | Type::JSONB => match row.try_get::<_, Option<JsonValue>>(idx) {
            Ok(v) => v.unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        // Timestamps / dates require chrono FromSql (with-chrono-0_4). Reading as
        // String always fails → previously rendered as NULL.
        Type::TIMESTAMP => match row.try_get::<_, Option<NaiveDateTime>>(idx) {
            Ok(v) => v
                .map(|dt| JsonValue::String(fmt_naive_dt(dt)))
                .unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::TIMESTAMPTZ => match row.try_get::<_, Option<DateTime<Utc>>>(idx) {
            Ok(v) => v
                .map(|dt| JsonValue::String(dt.to_rfc3339()))
                .unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::DATE => match row.try_get::<_, Option<NaiveDate>>(idx) {
            Ok(v) => v
                .map(|d| JsonValue::String(d.format("%Y-%m-%d").to_string()))
                .unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::TIME => match row.try_get::<_, Option<NaiveTime>>(idx) {
            Ok(v) => v
                .map(|t| JsonValue::String(fmt_naive_time(t)))
                .unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::NUMERIC => {
            // Prefer text form so precision is preserved.
            match row.try_get::<_, Option<String>>(idx) {
                Ok(v) => opt_string(v),
                Err(_) => match row.try_get::<_, Option<f64>>(idx) {
                    Ok(v) => v.map(|n| json!(n)).unwrap_or(JsonValue::Null),
                    Err(_) => JsonValue::Null,
                },
            }
        }
        _ => match row.try_get::<_, Option<String>>(idx) {
            Ok(v) => opt_string(v),
            Err(_) => {
                // Last resort: chrono types (domains / unknown aliases) + bytes.
                if let Ok(v) = row.try_get::<_, Option<DateTime<Utc>>>(idx) {
                    return v
                        .map(|dt| JsonValue::String(dt.to_rfc3339()))
                        .unwrap_or(JsonValue::Null);
                }
                if let Ok(v) = row.try_get::<_, Option<NaiveDateTime>>(idx) {
                    return v
                        .map(|dt| JsonValue::String(fmt_naive_dt(dt)))
                        .unwrap_or(JsonValue::Null);
                }
                if let Ok(v) = row.try_get::<_, Option<NaiveDate>>(idx) {
                    return v
                        .map(|d| JsonValue::String(d.format("%Y-%m-%d").to_string()))
                        .unwrap_or(JsonValue::Null);
                }
                if let Ok(v) = row.try_get::<_, Option<NaiveTime>>(idx) {
                    return v
                        .map(|t| JsonValue::String(fmt_naive_time(t)))
                        .unwrap_or(JsonValue::Null);
                }
                match row.try_get::<_, Option<Vec<u8>>>(idx) {
                    Ok(Some(b)) => {
                        // UUID binary / text-as-bytes etc.
                        if let Ok(s) = std::str::from_utf8(&b) {
                            JsonValue::String(s.to_string())
                        } else {
                            JsonValue::String(format!("\\x{}", encode_hex(&b)))
                        }
                    }
                    Ok(None) => JsonValue::Null,
                    Err(_) => JsonValue::Null,
                }
            }
        },
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[async_trait]
impl ConnectorSession for PostgresSession {
    fn engine_id(&self) -> &str {
        "postgres"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::postgres_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;
        client
            .simple_query("SELECT 1")
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        Ok(())
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        let status = self.pool.status();
        Some(PoolStatus {
            engine: "postgres".into(),
            max_size: self.max_size,
            active: status.size.saturating_sub(status.available) as u32,
            idle: status.available as u32,
            size: status.size as u32,
        })
    }

    async fn close(&self) -> ConnectorResult<()> {
        if self
            .closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.pool.close();
        }
        Ok(())
    }

    async fn execute_query(&self, query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }

        // Multi-statement batches: run each in order; return only the last result.
        let start = Instant::now();
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;

        let timeout = query.timeout_ms.max(1);
        let _ = client
            .batch_execute(&format!("SET statement_timeout = {timeout}"))
            .await;

        let stmts = split_sql_statements(&query.text);
        if stmts.is_empty() {
            return Err(ConnectorError::Query("Query is empty.".into()));
        }

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
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;

                let truncated = rows.len() > limit;
                let take = rows.len().min(limit);
                let slice = &rows[..take];

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
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;

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
        }

        let mut out = last.ok_or_else(|| ConnectorError::Query("No statements to run.".into()))?;
        out.duration_ms = start.elapsed().as_millis() as u64;
        Ok(out)
    }

    async fn describe_table(
        &self,
        _database: Option<&str>,
        schema: Option<&str>,
        table: &str,
    ) -> ConnectorResult<Vec<ColumnInfo>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let schema = schema.unwrap_or("public");
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;

        let sql = r#"
            SELECT
              c.column_name,
              c.data_type,
              c.udt_name,
              c.character_maximum_length,
              c.numeric_precision,
              c.numeric_scale,
              c.datetime_precision,
              c.is_nullable,
              (
                SELECT string_agg(e.enumlabel, E'\x1f' ORDER BY e.enumsortorder)
                FROM pg_catalog.pg_type t
                JOIN pg_catalog.pg_enum e ON e.enumtypid = t.oid
                JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
                WHERE t.typname = c.udt_name
                  AND (n.nspname = c.udt_schema OR c.udt_schema IS NULL)
              ) AS enum_labels,
              EXISTS (
                SELECT 1
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                  ON tc.constraint_name = kcu.constraint_name
                 AND tc.table_schema = kcu.table_schema
                WHERE tc.constraint_type = 'PRIMARY KEY'
                  AND tc.table_schema = c.table_schema
                  AND tc.table_name = c.table_name
                  AND kcu.column_name = c.column_name
              ) AS is_pk,
              c.column_default
            FROM information_schema.columns c
            WHERE c.table_schema = $1 AND c.table_name = $2
            ORDER BY c.ordinal_position
        "#;

        let rows = client
            .query(sql, &[&schema, &table])
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let name: String = row.get(0);
            let data_type: String = row.get(1);
            let udt_name: String = row.get(2);
            let char_max: Option<i32> = row.get(3);
            let num_prec: Option<i32> = row.get(4);
            let num_scale: Option<i32> = row.get(5);
            let dt_prec: Option<i32> = row.get(6);
            let is_nullable: String = row.get(7);
            let enum_labels: Option<String> = row.get(8);
            let is_pk: bool = row.get(9);
            let column_default: Option<String> = row.get(10);

            let enum_values = enum_labels.and_then(|labels| {
                let vals: Vec<String> = if labels.contains('\u{001f}') {
                    labels
                        .split('\u{001f}')
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                } else {
                    labels
                        .trim_matches(|c| c == '{' || c == '}')
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                };
                if vals.is_empty() {
                    None
                } else {
                    Some(vals)
                }
            });

            out.push(ColumnInfo {
                name,
                data_type: pg_type_label(
                    &udt_name,
                    &data_type,
                    pg_opt_i32(char_max),
                    pg_opt_i32(num_prec),
                    pg_opt_i32(num_scale),
                    pg_opt_i32(dt_prec),
                ),
                nullable: is_nullable.to_ascii_uppercase() != "NO",
                pk: is_pk,
                enum_values,
                default_value: column_default
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                references: None,
            });
        }

        // Attach single-column FK targets (plugin-owned navigation for the grid).
        if let Ok(fk_rows) = client
            .query(
                r#"
                SELECT
                  kcu.column_name,
                  ccu.table_schema,
                  ccu.table_name,
                  ccu.column_name
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                  ON tc.constraint_name = kcu.constraint_name
                 AND tc.table_schema = kcu.table_schema
                JOIN information_schema.constraint_column_usage ccu
                  ON ccu.constraint_name = tc.constraint_name
                 AND ccu.table_schema = tc.table_schema
                WHERE tc.constraint_type = 'FOREIGN KEY'
                  AND tc.table_schema = $1
                  AND tc.table_name = $2
                ORDER BY tc.constraint_name, kcu.ordinal_position
                "#,
                &[&schema, &table],
            )
            .await
        {
            use std::collections::HashMap;
            let mut by_col: HashMap<String, ColumnReference> = HashMap::new();
            for row in fk_rows {
                let from_col: String = row.get(0);
                // First FK segment wins for multi-column keys (still navigable).
                by_col.entry(from_col).or_insert(ColumnReference {
                    database: None,
                    schema: row.get::<_, Option<String>>(1),
                    table: row.get(2),
                    column: row.get(3),
                });
            }
            for col in &mut out {
                if let Some(r) = by_col.remove(&col.name) {
                    col.references = Some(r);
                }
            }
        }

        Ok(out)
    }

    async fn list_catalog_columns(
        &self,
        _database: Option<&str>,
        schema: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogColumnRow>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;

        let (sql, params_schema): (&str, Option<String>) = if let Some(s) = schema {
            (
                r#"
                SELECT c.table_schema, c.table_name, c.column_name,
                       c.data_type, c.udt_name,
                       c.character_maximum_length, c.numeric_precision,
                       c.numeric_scale, c.datetime_precision, c.is_nullable,
                       EXISTS (
                         SELECT 1
                         FROM information_schema.table_constraints tc
                         JOIN information_schema.key_column_usage kcu
                           ON tc.constraint_name = kcu.constraint_name
                          AND tc.table_schema = kcu.table_schema
                         WHERE tc.constraint_type = 'PRIMARY KEY'
                           AND tc.table_schema = c.table_schema
                           AND tc.table_name = c.table_name
                           AND kcu.column_name = c.column_name
                       ) AS is_pk
                FROM information_schema.columns c
                WHERE c.table_schema = $1
                ORDER BY c.table_schema, c.table_name, c.ordinal_position
                "#,
                Some(s.to_string()),
            )
        } else {
            (
                r#"
                SELECT c.table_schema, c.table_name, c.column_name,
                       c.data_type, c.udt_name,
                       c.character_maximum_length, c.numeric_precision,
                       c.numeric_scale, c.datetime_precision, c.is_nullable,
                       EXISTS (
                         SELECT 1
                         FROM information_schema.table_constraints tc
                         JOIN information_schema.key_column_usage kcu
                           ON tc.constraint_name = kcu.constraint_name
                          AND tc.table_schema = kcu.table_schema
                         WHERE tc.constraint_type = 'PRIMARY KEY'
                           AND tc.table_schema = c.table_schema
                           AND tc.table_name = c.table_name
                           AND kcu.column_name = c.column_name
                       ) AS is_pk
                FROM information_schema.columns c
                WHERE c.table_schema NOT IN ('pg_catalog', 'information_schema')
                ORDER BY c.table_schema, c.table_name, c.ordinal_position
                "#,
                None,
            )
        };

        let rows = if let Some(ref sch) = params_schema {
            client
                .query(sql, &[sch])
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?
        } else {
            client
                .query(sql, &[])
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?
        };

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let table_schema: String = row.get(0);
            let table_name: String = row.get(1);
            let column_name: String = row.get(2);
            let data_type: String = row.get(3);
            let udt_name: String = row.get(4);
            let char_max: Option<i32> = row.get(5);
            let num_prec: Option<i32> = row.get(6);
            let num_scale: Option<i32> = row.get(7);
            let dt_prec: Option<i32> = row.get(8);
            let is_nullable: String = row.get(9);
            let is_pk: bool = row.get(10);

            out.push(CatalogColumnRow {
                database: None,
                schema: Some(table_schema),
                table: table_name,
                column: column_name,
                data_type: pg_type_label(
                    &udt_name, &data_type, char_max, num_prec, num_scale, dt_prec,
                ),
                nullable: is_nullable.to_ascii_uppercase() != "NO",
                pk: is_pk,
            });
        }
        Ok(out)
    }

    async fn apply_table_edits(
        &self,
        edits: &ApplyTableEditsInput,
    ) -> ConnectorResult<ApplyTableEditsResult> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;

        let mut updated = 0u32;
        let mut inserted = 0u32;
        let mut deleted = 0u32;
        for u in &edits.updates {
            let sql = compile_update(SqlIdentStyle::Postgres, edits, u)?;
            client
                .batch_execute(&sql)
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            updated += 1;
        }
        for i in &edits.inserts {
            let sql = compile_insert(SqlIdentStyle::Postgres, edits, i)?;
            client
                .batch_execute(&sql)
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            inserted += 1;
        }
        for d in &edits.deletes {
            let sql = compile_delete(SqlIdentStyle::Postgres, edits, d)?;
            client
                .batch_execute(&sql)
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
            SqlIdentStyle::Postgres,
            input.database.as_deref(),
            input.schema.as_deref(),
            &input.table,
        );
        let order = preview_sort_sql(
            SqlIdentStyle::Postgres,
            &columns,
            input.sort_column.as_deref(),
            input.sort_dir.as_deref(),
        )
        .unwrap_or_else(|| preview_order_sql(SqlIdentStyle::Postgres, &columns));
        let limit = input.max_rows as i64;
        let select_list = preview_select_list(SqlIdentStyle::Postgres, &columns);
        let offset = input.offset;
        let where_sql = sql_where_clause(SqlIdentStyle::Postgres, &input.filters)?;
        let sql = format!(
            "SELECT {select_list} FROM {target}{where_sql}{order} LIMIT {limit} OFFSET {offset}"
        );

        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;
        let total_rows: i64 = client
            .query_one(&format!("SELECT COUNT(*) FROM {target}{where_sql}"), &[])
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?
            .get(0);
        let rows = client
            .query(sql.as_str(), &[])
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;

        let truncated = (input.offset as i64) + (rows.len() as i64) < total_rows;
        let take = rows.len().min(input.max_rows as usize);
        let mut out_rows = Vec::with_capacity(take);
        for row in rows.iter().take(take) {
            let mut cells = Vec::with_capacity(row.len());
            for j in 0..row.len() {
                cells.push(cell_json(row, j));
            }
            out_rows.push(cells);
        }
        let row_count = out_rows.len() as u64;
        Ok(TablePreviewResult {
            engine: "postgres".into(),
            columns,
            rows: out_rows,
            row_count,
            total_rows: total_rows.max(0) as u64,
            truncated,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn list_catalog(
        &self,
        preferred_database: Option<&str>,
        include_objects: bool,
    ) -> ConnectorResult<Vec<CatalogDatabase>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;
        let db_row = client
            .query_one("SELECT current_database()", &[])
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        let current: String = db_row.get(0);

        // Postgres connections are per-database; still list every connectable
        // database on the server so the explorer picker can switch.
        let db_rows = client
            .query(
                r#"
                SELECT datname
                FROM pg_database
                WHERE datallowconn
                  AND NOT datistemplate
                  AND has_database_privilege(datname, 'CONNECT')
                ORDER BY datname
                "#,
                &[],
            )
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        let mut names: Vec<String> = db_rows.iter().map(|row| row.get(0)).collect();
        if !names.iter().any(|n| n == &current) {
            names.push(current.clone());
        }
        if let Some(pref) = preferred_database.map(str::trim).filter(|s| !s.is_empty()) {
            if !names.iter().any(|n| n == pref) {
                names.push(pref.to_string());
            }
        }

        let mut current_objects = Vec::new();
        if include_objects {
            let table_rows = client
                .query(
                    r#"
                    SELECT table_schema, table_name, table_type
                    FROM information_schema.tables
                    WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
                    ORDER BY table_schema, table_name
                    "#,
                    &[],
                )
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            for row in table_rows {
                let schema: String = row.get(0);
                let name: String = row.get(1);
                let raw_type: String = row.get(2);
                let kind = if raw_type.to_ascii_lowercase().contains("view") {
                    "view"
                } else {
                    "table"
                };
                current_objects.push(CatalogObject {
                    name,
                    schema: Some(schema),
                    database: Some(current.clone()),
                    kind: kind.into(),
                    row_count: 0,
                });
            }
        }

        let pref = preferred_database.map(str::trim).unwrap_or("");
        names.sort_by(|a, b| {
            if !pref.is_empty() && a == pref {
                return std::cmp::Ordering::Less;
            }
            if !pref.is_empty() && b == pref {
                return std::cmp::Ordering::Greater;
            }
            if a == &current {
                return std::cmp::Ordering::Less;
            }
            if b == &current {
                return std::cmp::Ordering::Greater;
            }
            a.cmp(b)
        });

        Ok(names
            .into_iter()
            .map(|name| {
                let is_current = name == current;
                let objects = if is_current {
                    current_objects.clone()
                } else {
                    Vec::new()
                };
                let meta = if is_current {
                    if objects.is_empty() {
                        Some("empty".into())
                    } else {
                        Some(format!("{} tables", objects.len()))
                    }
                } else {
                    None
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
        _database: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogRelationship>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;
        let rows = client
            .query(
                r#"
                SELECT
                  kcu.table_schema AS from_schema,
                  kcu.table_name AS from_table,
                  kcu.column_name AS from_column,
                  ccu.table_schema AS to_schema,
                  ccu.table_name AS to_table,
                  ccu.column_name AS to_column,
                  tc.constraint_name
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                  ON tc.constraint_name = kcu.constraint_name
                 AND tc.table_schema = kcu.table_schema
                JOIN information_schema.constraint_column_usage ccu
                  ON ccu.constraint_name = tc.constraint_name
                 AND ccu.table_schema = tc.table_schema
                WHERE tc.constraint_type = 'FOREIGN KEY'
                ORDER BY kcu.table_schema, kcu.table_name
                "#,
                &[],
            )
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(CatalogRelationship {
                from_schema: row.get::<_, Option<String>>(0),
                from_table: row.get(1),
                from_column: row.get(2),
                to_schema: row.get::<_, Option<String>>(3),
                to_table: row.get(4),
                to_column: row.get(5),
                name: row.get::<_, Option<String>>(6),
            });
        }
        Ok(out)
    }

    async fn apply_table_admin(
        &self,
        input: &TableAdminInput,
    ) -> ConnectorResult<TableAdminResult> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;
        let schema = input
            .schema
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("public");
        let qname = qualify_table(
            SqlIdentStyle::Postgres,
            None,
            Some(schema),
            &input.name,
        );
        let is_view = input.kind == TableObjectKind::View;

        match input.op {
            TableAdminOp::CreateStatement => {
                let sql = pg_create_statement(&client, schema, &input.name, is_view).await?;
                Ok(TableAdminResult::statement(input.op, sql))
            }
            TableAdminOp::CreateGroup => {
                let qschema = quote_ident(SqlIdentStyle::Postgres, input.name.trim());
                let sql = format!("CREATE SCHEMA {qschema}");
                client
                    .batch_execute(&sql)
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                Ok(TableAdminResult::executed(
                    input.op,
                    format!("Created schema {qschema}."),
                ))
            }
            TableAdminOp::CreateObject => {
                // Minimal empty table with a PK so CREATE TABLE is valid.
                let sql = format!(
                    "CREATE TABLE {qname} (id bigserial PRIMARY KEY)"
                );
                client
                    .batch_execute(&sql)
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                Ok(TableAdminResult::executed(
                    input.op,
                    format!("Created table {qname}."),
                ))
            }
            TableAdminOp::Rename => {
                let new_name = input.new_name.as_deref().unwrap_or("").trim();
                let dest = quote_ident(SqlIdentStyle::Postgres, new_name);
                let kw = if is_view { "VIEW" } else { "TABLE" };
                let sql = format!("ALTER {kw} {qname} RENAME TO {dest}");
                client
                    .batch_execute(&sql)
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                Ok(TableAdminResult::executed(
                    input.op,
                    format!("Renamed {qname} to {dest}."),
                ))
            }
            TableAdminOp::Drop => {
                let kw = if is_view { "VIEW" } else { "TABLE" };
                let sql = format!("DROP {kw} IF EXISTS {qname}");
                client
                    .batch_execute(&sql)
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                Ok(TableAdminResult::executed(input.op, format!("Dropped {qname}.")))
            }
            TableAdminOp::Truncate => {
                if is_view {
                    return Err(ConnectorError::NotSupported(
                        "Cannot truncate a view.".into(),
                    ));
                }
                let sql = format!("TRUNCATE TABLE {qname}");
                client
                    .batch_execute(&sql)
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                Ok(TableAdminResult::executed(
                    input.op,
                    format!("Truncated {qname}."),
                ))
            }
            TableAdminOp::Duplicate => {
                if is_view {
                    return Err(ConnectorError::NotSupported(
                        "Duplicate is not supported for views.".into(),
                    ));
                }
                let new_name = input.new_name.as_deref().unwrap_or("").trim();
                let dest = qualify_table(
                    SqlIdentStyle::Postgres,
                    None,
                    Some(schema),
                    new_name,
                );
                let sql = format!(
                    "CREATE TABLE {dest} (LIKE {qname} INCLUDING ALL); INSERT INTO {dest} SELECT * FROM {qname}"
                );
                client
                    .batch_execute(&sql)
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                Ok(TableAdminResult::executed(
                    input.op,
                    format!("Duplicated {qname} to {dest}."),
                ))
            }
        }
    }

    async fn describe_object_structure(
        &self,
        _database: Option<&str>,
        schema: Option<&str>,
        table: &str,
        kind: Option<&str>,
    ) -> ConnectorResult<ObjectStructure> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let schema = schema.unwrap_or("public");
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;

        let cols = self
            .describe_table(None, Some(schema), table)
            .await?;
        let col_rows: Vec<Vec<String>> = cols
            .iter()
            .map(|c| {
                vec![
                    c.name.clone(),
                    c.data_type.clone(),
                    cell_bool(c.nullable),
                    cell_bool(c.pk),
                    c.default_value
                        .clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "—".into()),
                    c.enum_values
                        .as_ref()
                        .map(|v| v.join(", "))
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "—".into()),
                ]
            })
            .collect();
        let mut columns_sec = StructureSection::table(
            "columns",
            "Columns",
            vec![
                "Name".into(),
                "Type".into(),
                "Nullable".into(),
                "PK".into(),
                "Default".into(),
                "Enum".into(),
            ],
            col_rows,
        );
        if columns_sec.rows.is_empty() {
            columns_sec = columns_sec.with_empty_message("No columns found.");
        }

        let idx_rows = client
            .query(
                r#"
                SELECT
                  i.relname::text,
                  ix.indisunique,
                  ix.indisprimary,
                  pg_get_indexdef(ix.indexrelid)
                FROM pg_class t
                JOIN pg_index ix ON t.oid = ix.indrelid
                JOIN pg_class i ON i.oid = ix.indexrelid
                JOIN pg_namespace n ON n.oid = t.relnamespace
                WHERE n.nspname = $1 AND t.relname = $2
                ORDER BY i.relname
                "#,
                &[&schema, &table],
            )
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        let index_rows: Vec<Vec<String>> = idx_rows
            .iter()
            .map(|r| {
                vec![
                    r.get::<_, String>(0),
                    cell_bool(r.get::<_, bool>(1)),
                    cell_bool(r.get::<_, bool>(2)),
                    r.get::<_, String>(3),
                ]
            })
            .collect();
        let mut indexes_sec = StructureSection::table(
            "indexes",
            "Indexes",
            vec![
                "Name".into(),
                "Unique".into(),
                "Primary".into(),
                "Definition".into(),
            ],
            index_rows,
        )
        .with_empty_message("No indexes.");

        let fk_rows = client
            .query(
                r#"
                SELECT
                  tc.constraint_name,
                  kcu.column_name,
                  ccu.table_schema,
                  ccu.table_name,
                  ccu.column_name
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                  ON tc.constraint_name = kcu.constraint_name
                 AND tc.table_schema = kcu.table_schema
                JOIN information_schema.constraint_column_usage ccu
                  ON ccu.constraint_name = tc.constraint_name
                 AND ccu.table_schema = tc.table_schema
                WHERE tc.constraint_type = 'FOREIGN KEY'
                  AND tc.table_schema = $1
                  AND tc.table_name = $2
                ORDER BY tc.constraint_name, kcu.ordinal_position
                "#,
                &[&schema, &table],
            )
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        let fk_display: Vec<Vec<String>> = fk_rows
            .iter()
            .map(|r| {
                let to_schema: String = r.get(2);
                let to_table: String = r.get(3);
                vec![
                    r.get::<_, String>(0),
                    r.get::<_, String>(1),
                    format!("{to_schema}.{to_table}"),
                    r.get::<_, String>(4),
                ]
            })
            .collect();
        let mut fks_sec = StructureSection::table(
            "foreign_keys",
            "Foreign keys",
            vec![
                "Constraint".into(),
                "Column".into(),
                "References".into(),
                "Ref column".into(),
            ],
            fk_display,
        )
        .with_empty_message("No foreign keys.");

        let inbound = client
            .query(
                r#"
                SELECT
                  tc.constraint_name,
                  kcu.table_schema,
                  kcu.table_name,
                  kcu.column_name,
                  ccu.column_name
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                  ON tc.constraint_name = kcu.constraint_name
                 AND tc.table_schema = kcu.table_schema
                JOIN information_schema.constraint_column_usage ccu
                  ON ccu.constraint_name = tc.constraint_name
                 AND ccu.table_schema = tc.table_schema
                WHERE tc.constraint_type = 'FOREIGN KEY'
                  AND ccu.table_schema = $1
                  AND ccu.table_name = $2
                ORDER BY kcu.table_schema, kcu.table_name
                "#,
                &[&schema, &table],
            )
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        let inbound_display: Vec<Vec<String>> = inbound
            .iter()
            .map(|r| {
                let from_schema: String = r.get(1);
                let from_table: String = r.get(2);
                vec![
                    r.get::<_, String>(0),
                    format!("{from_schema}.{from_table}"),
                    r.get::<_, String>(3),
                    r.get::<_, String>(4),
                ]
            })
            .collect();
        let mut refs_sec = StructureSection::table(
            "referenced_by",
            "Referenced by",
            vec![
                "Constraint".into(),
                "From table".into(),
                "From column".into(),
                "To column".into(),
            ],
            inbound_display,
        )
        .with_empty_message("No inbound foreign keys.");

        let trig_rows = client
            .query(
                r#"
                SELECT
                  trigger_name,
                  action_timing,
                  event_manipulation,
                  action_statement
                FROM information_schema.triggers
                WHERE event_object_schema = $1
                  AND event_object_table = $2
                ORDER BY trigger_name, event_manipulation
                "#,
                &[&schema, &table],
            )
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        let trigger_display: Vec<Vec<String>> = trig_rows
            .iter()
            .map(|r| {
                vec![
                    r.get::<_, String>(0),
                    r.get::<_, String>(1),
                    r.get::<_, String>(2),
                    r.get::<_, String>(3),
                ]
            })
            .collect();
        let mut triggers_sec = StructureSection::table(
            "triggers",
            "Triggers",
            vec![
                "Name".into(),
                "Timing".into(),
                "Event".into(),
                "Action".into(),
            ],
            trigger_display,
        )
        .with_empty_message("No triggers.");

        let check_rows = client
            .query(
                r#"
                SELECT
                  tc.constraint_name,
                  cc.check_clause
                FROM information_schema.table_constraints tc
                JOIN information_schema.check_constraints cc
                  ON tc.constraint_name = cc.constraint_name
                 AND tc.constraint_schema = cc.constraint_schema
                WHERE tc.constraint_type = 'CHECK'
                  AND tc.table_schema = $1
                  AND tc.table_name = $2
                ORDER BY tc.constraint_name
                "#,
                &[&schema, &table],
            )
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        let check_display: Vec<Vec<String>> = check_rows
            .iter()
            .map(|r| vec![r.get::<_, String>(0), r.get::<_, String>(1)])
            .collect();
        let mut checks_sec = StructureSection::table(
            "checks",
            "Check constraints",
            vec!["Name".into(), "Clause".into()],
            check_display,
        )
        .with_empty_message("No check constraints.");

        let title = format!("{schema}.{table}");
        let object_kind = kind.unwrap_or("table").to_string();
        Ok(ObjectStructure::new(
            title,
            vec![
                columns_sec,
                indexes_sec,
                fks_sec,
                refs_sec,
                triggers_sec,
                checks_sec,
            ],
        )
        .with_subtitle(format!("PostgreSQL · {object_kind}"))
        .with_object_kind(object_kind))
    }
}

async fn pg_create_statement(
    client: &deadpool_postgres::Object,
    schema: &str,
    name: &str,
    prefer_view: bool,
) -> ConnectorResult<String> {
    let found = client
        .query_opt(
            r#"
            SELECT c.oid, c.relkind::text
            FROM pg_catalog.pg_class c
            JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = $1 AND c.relname = $2
            "#,
            &[&schema, &name],
        )
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;
    let Some(row) = found else {
        return Err(ConnectorError::Query(format!(
            "Object `{schema}.{name}` was not found."
        )));
    };
    let oid: u32 = row.get(0);
    let relkind: String = row.get(1);
    let is_view = prefer_view || relkind == "v" || relkind == "m";
    let qname = qualify_table(SqlIdentStyle::Postgres, None, Some(schema), name);

    if is_view {
        let def: String = client
            .query_one("SELECT pg_get_viewdef($1::oid, true)", &[&oid])
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?
            .get(0);
        let kw = if relkind == "m" {
            "MATERIALIZED VIEW"
        } else {
            "VIEW"
        };
        return Ok(format!("CREATE {kw} {qname} AS\n{def}\n"));
    }

    let cols = client
        .query(
            r#"
            SELECT a.attname,
                   pg_catalog.format_type(a.atttypid, a.atttypmod),
                   a.attnotnull
            FROM pg_catalog.pg_attribute a
            WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped
            ORDER BY a.attnum
            "#,
            &[&oid],
        )
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;
    let mut lines: Vec<String> = cols
        .iter()
        .map(|c| {
            let col: String = c.get(0);
            let typ: String = c.get(1);
            let not_null: bool = c.get(2);
            let nn = if not_null { " NOT NULL" } else { "" };
            format!(
                "  {} {typ}{nn}",
                quote_ident(SqlIdentStyle::Postgres, &col)
            )
        })
        .collect();
    let pk = client
        .query_opt(
            "SELECT pg_get_constraintdef(oid) FROM pg_catalog.pg_constraint WHERE conrelid = $1 AND contype = 'p'",
            &[&oid],
        )
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;
    if let Some(row) = pk {
        let def: String = row.get(0);
        lines.push(format!("  {def}"));
    }
    if lines.is_empty() {
        lines.push("  -- (no columns)".into());
    }
    Ok(format!(
        "CREATE TABLE {qname} (\n{}\n);\n",
        lines.join(",\n")
    ))
}

fn preview_select_list(style: SqlIdentStyle, columns: &[ColumnInfo]) -> String {
    if columns.is_empty() {
        return "*".into();
    }
    columns
        .iter()
        .map(|c| quote_ident(style, &c.name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn preview_order_sql(style: SqlIdentStyle, columns: &[ColumnInfo]) -> String {
    let pks: Vec<String> = columns
        .iter()
        .filter(|c| c.pk)
        .map(|c| quote_ident(style, &c.name))
        .collect();
    if !pks.is_empty() {
        return format!(" ORDER BY {}", pks.join(", "));
    }
    if let Some(first) = columns.first() {
        return format!(" ORDER BY {}", quote_ident(style, &first.name));
    }
    String::new()
}
