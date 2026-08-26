//! Policy-validated SQL execution for a pooled SQLite session.

use std::time::Instant;

use mystique_connector_api::{
    split_sql_statements, ConnectorError, ConnectorResult, QueryColumn, QueryResult, ValidatedQuery,
};

use super::encode::value_ref_json;
use super::SqliteSession;

pub(super) async fn execute_query(
    session: &SqliteSession,
    query: &ValidatedQuery,
) -> ConnectorResult<QueryResult> {
    session.ensure_open()?;
    // Multi-statement batches: run each in order; return only the last result.
    let start = Instant::now();
    let pool = session.pool();
    let max_rows = query.max_rows as usize;
    let mode = query.mode;
    let stmts = split_sql_statements(&query.text);
    if stmts.is_empty() {
        return Err(ConnectorError::Query("Query is empty.".into()));
    }

    let result = session
        .blocking(move || -> ConnectorResult<QueryResult> {
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
        .await?;

    Ok(QueryResult {
        duration_ms: start.elapsed().as_millis() as u64,
        ..result
    })
}
