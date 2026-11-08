#![allow(dead_code)]

use mystique_connector_api::{
    gate_sql_batch_in, ConnectionConfig, ConnectorSession, DatabaseConnector, PoolConfig,
    QueryBatchResult, QueryControl, QueryMode, ValidatedBatch, ValidatedQuery,
};
use mystique_plugin_mysql::MysqlConnector;
use mystique_sql::SqlDialect;
use serde_json::json;

pub fn local_config() -> ConnectionConfig {
    ConnectionConfig {
        engine: "mysql".into(),
        plugin_config: json!({
            "host": "127.0.0.1",
            "port": 3306,
            "database": "mystique",
            "username": "admin"
        }),
        password: Some("123".into()),
        pool: PoolConfig {
            max_size: 1,
            min_idle: 0,
            acquire_timeout_ms: 5000,
            idle_timeout_secs: 30,
        },
    }
}

/// One pooled connection, so every call reuses the same server session.
pub async fn connect() -> Box<dyn ConnectorSession> {
    MysqlConnector
        .connect(&local_config())
        .await
        .expect("connect")
}

pub async fn run(
    session: &dyn ConnectorSession,
    sql: &str,
    mode: QueryMode,
) -> Result<mystique_connector_api::QueryResult, String> {
    session
        .execute_query(&ValidatedQuery::new(sql, mode, 100, 10_000))
        .await
        .map_err(|err| err.to_string())
}

/// Plan as Write (so the classifier lets everything through), then run in
/// `mode`. This tests the server-side lock, not the classifier.
pub async fn run_batch(
    session: &dyn ConnectorSession,
    sql: &str,
    mode: QueryMode,
    max_rows: u32,
) -> QueryBatchResult {
    let plan =
        gate_sql_batch_in(sql, SqlDialect::Mysql, QueryMode::Write, true, &[]).expect("plan");
    session
        .execute_batch(&ValidatedBatch {
            statements: plan.statements,
            mode,
            max_rows,
            timeout_ms: 10_000,
            explain_executes: plan.explain_executes,
            collection: None,
            database: None,
            control: QueryControl::new(),
            progress: None,
        })
        .await
        .expect("batch")
}

pub async fn write(session: &dyn ConnectorSession, sql: &str) {
    run(session, sql, QueryMode::Write)
        .await
        .unwrap_or_else(|err| panic!("{sql}: {err}"));
}

pub async fn count(session: &dyn ConnectorSession, sql: &str) -> i64 {
    let result = run(session, sql, QueryMode::Write)
        .await
        .unwrap_or_else(|err| panic!("{sql}: {err}"));
    let cell = &result.rows[0][0];
    cell.as_i64()
        .or_else(|| cell.as_str().and_then(|text| text.parse().ok()))
        .unwrap_or_else(|| panic!("count cell {cell}"))
}
