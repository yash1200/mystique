//! Grid insert / update / delete. One transaction, one statement per row.

use mysql_async::prelude::*;
use mysql_async::Value;
use mystique_connector_api::{
    affected_mismatch, keyless_ambiguous, overlay_catalog_pk, rollback_stale, rows_hash,
    ApplyTableEditsInput, ApplyTableEditsResult, ConnectorError, ConnectorResult, GridUndo,
};
use mystique_sql::dml::{plan_table_edits, EditPlan};
use mystique_sql::{SqlDialect, SqlPlaceholder};
use serde_json::Value as JsonValue;

use super::catalog;
use super::encode::value_json;
use super::query_failed;
use super::MysqlSession;

pub(super) async fn apply_table_edits(
    session: &MysqlSession,
    edits: &ApplyTableEditsInput,
) -> ConnectorResult<ApplyTableEditsResult> {
    session.ensure_open()?;
    let described = catalog::describe_table(
        session,
        edits.database.as_deref(),
        edits.schema.as_deref(),
        &edits.table,
    )
    .await?;
    let mut owned = edits.clone();
    overlay_catalog_pk(&mut owned.columns, &described);
    let plan = plan_table_edits(SqlDialect::Mysql, SqlPlaceholder::Question, &owned)?;
    let previews = plan.previews();
    if edits.dry_run {
        return Ok(ApplyTableEditsResult {
            statements: previews,
            ..ApplyTableEditsResult::empty()
        });
    }
    let mut conn = session.edit_conn().await?;
    conn.query_drop("START TRANSACTION")
        .await
        .map_err(query_failed)?;
    if let Err(err) = apply_plan(&mut conn, &plan).await {
        let _ = conn.query_drop("ROLLBACK").await;
        return Err(err);
    }
    let undo = read_undo(&mut conn, &plan).await;
    if let Err(err) = conn.query_drop("COMMIT").await {
        let _ = conn.query_drop("ROLLBACK").await;
        return Err(query_failed(err));
    }
    Ok(ApplyTableEditsResult {
        updated: owned.updates.len() as u32,
        inserted: owned.inserts.len() as u32,
        deleted: owned.deletes.len() as u32,
        statements: previews,
        save_id: None,
        undo,
    })
}

pub(super) async fn apply_grid_undo(
    session: &MysqlSession,
    undo: &GridUndo,
) -> ConnectorResult<()> {
    session.ensure_open()?;
    let mut conn = session.edit_conn().await?;
    conn.query_drop("START TRANSACTION")
        .await
        .map_err(query_failed)?;
    if let Err(err) = run_undo(&mut conn, undo).await {
        let _ = conn.query_drop("ROLLBACK").await;
        return Err(err);
    }
    if let Err(err) = conn.query_drop("COMMIT").await {
        let _ = conn.query_drop("ROLLBACK").await;
        return Err(query_failed(err));
    }
    Ok(())
}

/// Post-save rows for the undo. `None` means the save has no rollback.
async fn read_undo(conn: &mut mysql_async::Conn, plan: &EditPlan) -> Option<GridUndo> {
    plan.rollback_sql()?;
    let mut rows = Vec::new();
    for edit in &plan.edits {
        rows.push(
            read_rows(conn, &edit.check.sql, &edit.check.params)
                .await
                .ok()?,
        );
    }
    plan.undo(&rows)
}

async fn run_undo(conn: &mut mysql_async::Conn, undo: &GridUndo) -> ConnectorResult<()> {
    for check in &undo.checks {
        let rows = read_rows(conn, &check.sql, &check.params).await?;
        if rows_hash(&rows) != check.hash {
            return Err(rollback_stale());
        }
    }
    for sql in &undo.inverse {
        conn.exec_drop(sql.as_str(), ())
            .await
            .map_err(query_failed)?;
        let changed = conn.affected_rows();
        if changed != 1 {
            return Err(affected_mismatch("rollback", changed));
        }
    }
    Ok(())
}

async fn read_rows(
    conn: &mut mysql_async::Conn,
    sql: &str,
    params: &[JsonValue],
) -> ConnectorResult<Vec<Vec<JsonValue>>> {
    let rows: Vec<mysql_async::Row> = conn
        .exec(sql, mysql_params(params))
        .await
        .map_err(query_failed)?;
    Ok(rows
        .into_iter()
        .map(|row| row.unwrap().into_iter().map(value_json).collect())
        .collect())
}

async fn apply_plan(conn: &mut mysql_async::Conn, plan: &EditPlan) -> ConnectorResult<()> {
    for edit in &plan.edits {
        if let Some(count) = &edit.uniqueness {
            let found = count_rows(conn, &count.sql, &count.params).await?;
            if found != 1 {
                return Err(if found > 1 {
                    keyless_ambiguous(found)
                } else {
                    affected_mismatch("write", 0)
                });
            }
        }
        let params = mysql_params(&edit.params);
        if let Err(err) = conn.exec_drop(edit.sql.as_str(), params).await {
            return Err(query_failed(err));
        }
        let changed = conn.affected_rows();
        if changed != 1 {
            return Err(affected_mismatch(statement_kind(&edit.sql), changed));
        }
    }
    Ok(())
}

async fn count_rows(
    conn: &mut mysql_async::Conn,
    sql: &str,
    params: &[JsonValue],
) -> ConnectorResult<i64> {
    let mut result = conn
        .exec_iter(sql, mysql_params(params))
        .await
        .map_err(query_failed)?;
    let row = result.next().await.map_err(query_failed)?;
    result.drop_result().await.map_err(query_failed)?;
    let Some(row) = row else {
        return Err(ConnectorError::Query(
            "The row count returned no row.".into(),
        ));
    };
    value_i64(row.as_ref(0).cloned().unwrap_or(Value::NULL))
}

pub(super) fn mysql_params(params: &[JsonValue]) -> Vec<Value> {
    params.iter().map(mysql_value).collect()
}

fn mysql_value(value: &JsonValue) -> Value {
    match value {
        JsonValue::Null => Value::NULL,
        JsonValue::Bool(flag) => Value::Int(if *flag { 1 } else { 0 }),
        JsonValue::Number(number) => {
            if let Some(integer) = number.as_i64() {
                Value::Int(integer)
            } else if let Some(unsigned) = number.as_u64() {
                Value::UInt(unsigned)
            } else if let Some(real) = number.as_f64() {
                Value::Double(real)
            } else {
                Value::from(number.to_string())
            }
        }
        JsonValue::String(text) => Value::from(text.clone()),
        other => Value::from(other.to_string()),
    }
}

fn value_i64(value: Value) -> ConnectorResult<i64> {
    match value {
        Value::Int(number) => Ok(number),
        Value::UInt(number) => i64::try_from(number).map_err(|_| {
            ConnectorError::Query(format!("Row count {number} does not fit a signed integer."))
        }),
        Value::Bytes(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            text.trim()
                .parse::<i64>()
                .map_err(|_| ConnectorError::Query(format!("Row count was {text}.")))
        }
        _ => Err(ConnectorError::Query(
            "Row count came back in an unexpected type.".into(),
        )),
    }
}

fn statement_kind(sql: &str) -> &'static str {
    let trimmed = sql.trim_start();
    if trimmed.starts_with("INSERT") || trimmed.starts_with("insert") {
        "INSERT"
    } else if trimmed.starts_with("DELETE") || trimmed.starts_with("delete") {
        "DELETE"
    } else {
        "UPDATE"
    }
}
