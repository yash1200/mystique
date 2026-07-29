use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use deadpool_postgres::Pool;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use mystique_connector_api::{
    split_sql_statements, ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities,
    PoolStatus, QueryColumn, QueryMode, QueryResult, ValidatedQuery,
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

/// Format naive timestamp without trailing empty fractional part.
fn fmt_naive_dt(dt: NaiveDateTime) -> String {
    let s = dt.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
    s.trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn fmt_naive_time(t: NaiveTime) -> String {
    let s = t.format("%H:%M:%S%.6f").to_string();
    s.trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
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
}
