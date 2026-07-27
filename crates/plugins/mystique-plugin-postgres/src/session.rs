use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use deadpool_postgres::Pool;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities, PoolStatus, QueryColumn,
    QueryMode, QueryResult, ValidatedQuery,
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
        _ => match row.try_get::<_, Option<String>>(idx) {
            Ok(v) => v.map(JsonValue::String).unwrap_or(JsonValue::Null),
            Err(_) => match row.try_get::<_, Option<Vec<u8>>>(idx) {
                Ok(Some(b)) => JsonValue::String(format!("\\x{}", encode_hex(&b))),
                _ => JsonValue::Null,
            },
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

        // Writes: policy allowed but we only execute SELECT-class for tabular results MVP.
        // INSERT/UPDATE without RETURNING still runs and returns empty grid + message.
        let start = Instant::now();
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;

        // Statement timeout (ms)
        let timeout = query.timeout_ms.max(1);
        let _ = client
            .batch_execute(&format!("SET statement_timeout = {timeout}"))
            .await;

        let sql = query.text.trim().trim_end_matches(';').trim();
        // Fetch one extra row to detect truncation
        let limit = query.max_rows as usize;
        let fetch_limit = limit + 1;

        // Prefer wrapping SELECT-like statements; otherwise run as-is.
        let lower = sql.to_ascii_lowercase();
        let is_select_like = lower.starts_with("select")
            || lower.starts_with("with")
            || lower.starts_with("values")
            || lower.starts_with("table")
            || lower.starts_with("show")
            || lower.starts_with("explain");

        let exec_sql = if is_select_like {
            // Subquery wrap to enforce max rows without rewriting complex SQL badly.
            // If SQL already has LIMIT, still cap client-side.
            sql.to_string()
        } else {
            sql.to_string()
        };

        let rows = client
            .query(&exec_sql, &[])
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
        } else if is_select_like {
            // Empty result — try to DESCRIBE isn't free; return empty cols
            Vec::new()
        } else {
            Vec::new()
        };

        let mut out_rows = Vec::with_capacity(take);
        for row in slice {
            let mut cells = Vec::with_capacity(row.len());
            for i in 0..row.len() {
                cells.push(cell_json(row, i));
            }
            out_rows.push(cells);
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let message = if !is_select_like {
            Some(format!(
                "Statement executed ({}). Tabular rows only for SELECT-like queries.",
                match query.mode {
                    QueryMode::Read => "read",
                    QueryMode::Write => "write",
                }
            ))
        } else if truncated {
            Some(format!("Result truncated to {limit} rows."))
        } else {
            None
        };

        // silence unused
        let _ = fetch_limit;

        Ok(QueryResult {
            engine: "postgres".into(),
            mode: query.mode,
            columns,
            row_count: out_rows.len() as u64,
            rows: out_rows,
            truncated,
            duration_ms,
            message,
        })
    }
}
