use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use mysql_async::prelude::*;
use mysql_async::{Pool, Value};
use mystique_connector_api::{
    split_sql_statements, ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities,
    PoolStatus, QueryColumn, QueryMode, QueryResult, ValidatedQuery,
};
use serde_json::{json, Value as JsonValue};

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
                JsonValue::String(format!(
                    "{sign}{days} {hours:02}:{minutes:02}:{seconds:02}"
                ))
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
                        data_type: Some(format!("{:?}", c.column_type())),
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
        assert!(tables.row_count >= 5, "expected seed tables, got {}", tables.row_count);
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
        assert!(preview.columns.iter().any(|c| c.name == "email" || c.name == "id"));

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
        let session = MysqlConnector
            .connect(&config)
            .await
            .expect("connect");

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
