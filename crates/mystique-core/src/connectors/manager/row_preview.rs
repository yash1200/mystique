//! "Preview affected rows" in the production write dialog (F-05).
//!
//! On demand only: the dialog asks, nothing runs before. The statement is
//! never executed. `mystique-sql` turns it into a `SELECT` (and a
//! `COUNT(*)`) with the same table and `WHERE`; both pass the Read-mode
//! policy and run in Read mode, inside the engine's read-only transaction.

use std::time::Duration;

use mystique_connector_api::{
    clip_query_result, ConnectorError, ConnectorResult, QueryColumn, QueryMode, SqlDialect,
    ValidatedQuery,
};
use mystique_connector_api::{insert_row_count, row_preview_plan, RowPreviewAction};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::ConnectionManager;

/// Rows shown at most.
pub const ROW_PREVIEW_LIMIT: u32 = 50;
const ROWS_TIMEOUT_MS: u64 = 10_000;
/// The count is a hint: it gives up quickly on a huge table.
const COUNT_TIMEOUT_MS: u64 = 3_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct PreviewWriteRowsRequest {
    pub session_id: String,
    /// One statement from the write dialog.
    pub statement: String,
    #[serde(default)]
    pub database: Option<String>,
}

/// The rows one write would touch, or why there is no preview.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct WriteRowPreview {
    /// False: `reason` says why there is no preview.
    pub available: bool,
    pub reason: Option<String>,
    /// `update` | `delete` | `insert`.
    pub action: Option<String>,
    pub table: Option<String>,
    pub columns: Vec<QueryColumn>,
    pub rows: Vec<Vec<JsonValue>>,
    /// UPDATE: the `"<column> (new)"` columns at the end of each row.
    pub new_columns: Vec<String>,
    /// How many rows the write touches. `None`: the count did not finish.
    pub total: Option<u64>,
    /// More rows than `rows` holds.
    pub truncated: bool,
    /// The read failed (timeout, permissions). The write is not allowed by it.
    pub error: Option<String>,
    /// Why `total` is missing.
    pub note: Option<String>,
}

impl WriteRowPreview {
    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            reason: Some(reason.into()),
            ..Self::default()
        }
    }
}

impl ConnectionManager {
    /// Preview the rows one statement would change. Runs reads only.
    pub async fn preview_write_rows(
        &self,
        req: PreviewWriteRowsRequest,
    ) -> ConnectorResult<WriteRowPreview> {
        let meta = self.session_meta(&req.session_id).await?;
        let engine = meta.session.engine_id().to_string();
        let dialect = SqlDialect::from_engine(&engine);
        if engine.eq_ignore_ascii_case("mongodb") || dialect == SqlDialect::Generic {
            return Ok(WriteRowPreview::unavailable(
                "Row preview is for Postgres, MySQL and SQLite.",
            ));
        }
        let plan = match row_preview_plan(req.statement.trim(), dialect, ROW_PREVIEW_LIMIT) {
            Ok(plan) => plan,
            Err(reason) => return Ok(WriteRowPreview::unavailable(reason)),
        };
        let policy = self.registry.policy(&engine).ok_or_else(|| {
            ConnectorError::Internal(format!("no query policy registered for engine `{engine}`"))
        })?;
        for read in [plan.rows_sql.as_deref(), plan.count_sql.as_deref()]
            .into_iter()
            .flatten()
        {
            if policy.validate(read, QueryMode::Read).is_err() {
                return Ok(WriteRowPreview::unavailable(
                    "Row preview only runs reads, and this WHERE calls something that may write.",
                ));
            }
        }
        self.touch_session(&req.session_id);
        let mut out = WriteRowPreview {
            available: true,
            action: Some(plan.action.as_str().to_string()),
            table: Some(plan.table.clone()),
            new_columns: plan.new_columns.clone(),
            ..WriteRowPreview::default()
        };
        let read = |text: &str, max_rows: u32, timeout_ms: u64| {
            let mut query = ValidatedQuery::new(text, QueryMode::Read, max_rows, timeout_ms);
            query.database = req.database.clone();
            query
        };

        if plan.action == RowPreviewAction::Insert {
            let mut names = plan.insert_columns.clone();
            // No column list: the table's columns, in order. A catalog
            // failure leaves numbered columns rather than failing the preview.
            if let Some(path) = plan.insert_table.as_ref() {
                let current = self.session_database(&req.session_id).await;
                let database = path
                    .database
                    .as_deref()
                    .or(req.database.as_deref())
                    .or(current.as_deref());
                let described = tokio::time::timeout(
                    Duration::from_millis(ROWS_TIMEOUT_MS),
                    meta.session
                        .describe_table(database, path.schema.as_deref(), &path.table),
                )
                .await;
                if let Ok(Ok(columns)) = described {
                    names = columns.into_iter().map(|c| c.name).collect();
                }
            }
            let width = plan.insert_rows.iter().map(Vec::len).max().unwrap_or(0);
            out.columns = (0..width)
                .map(|i| QueryColumn {
                    name: names
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("column {}", i + 1)),
                    data_type: None,
                })
                .collect();
            out.rows = plan.insert_rows;
            out.total = insert_row_count(req.statement.trim(), dialect)
                .or(u64::try_from(out.rows.len()).ok());
            out.truncated = out.total.is_some_and(|total| total > out.rows.len() as u64);
            return Ok(out);
        }

        let Some(rows_sql) = plan.rows_sql.as_deref() else {
            return Ok(WriteRowPreview::unavailable("Nothing to preview."));
        };
        match run(
            &meta.session,
            &read(rows_sql, ROW_PREVIEW_LIMIT, ROWS_TIMEOUT_MS),
        )
        .await
        {
            Ok(mut result) => {
                clip_query_result(&mut result);
                out.columns = result.columns;
                out.rows = result.rows;
                out.truncated = result.truncated;
            }
            Err(err) => {
                out.error = Some(err.to_string());
                return Ok(out);
            }
        }
        if let Some(count_sql) = plan.count_sql.as_deref() {
            match run(&meta.session, &read(count_sql, 1, COUNT_TIMEOUT_MS)).await {
                Ok(result) => {
                    out.total = result
                        .rows
                        .first()
                        .and_then(|r| r.first())
                        .and_then(count_of)
                }
                Err(ConnectorError::Timeout(_)) => {
                    out.note = Some("Counting took longer than 3 seconds.".into());
                }
                Err(err) => out.note = Some(format!("Could not count the rows: {err}")),
            }
        }
        if let Some(total) = out.total {
            out.truncated = total > out.rows.len() as u64;
        }
        Ok(out)
    }
}

/// One read, with a client deadline on top of the engine's timeout.
async fn run(
    session: &std::sync::Arc<dyn mystique_connector_api::ConnectorSession>,
    query: &ValidatedQuery,
) -> ConnectorResult<mystique_connector_api::QueryResult> {
    debug_assert_eq!(query.mode, QueryMode::Read);
    let deadline = Duration::from_millis(query.timeout_ms.saturating_add(2_000));
    match tokio::time::timeout(deadline, session.execute_query(query)).await {
        Ok(result) => result,
        Err(_) => {
            query.control.cancel();
            Err(ConnectorError::Timeout(
                "The preview read timed out.".into(),
            ))
        }
    }
}

fn count_of(cell: &JsonValue) -> Option<u64> {
    match cell {
        JsonValue::Number(n) => n.as_u64(),
        JsonValue::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}
