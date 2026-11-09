//! Streaming reads on one connection: a whole table, or one bound statement.
//!
//! mysql_async reads rows off the soccolumn.pk = keys.contains(&name);et as they are pulled, so memory is
//! one batch. One statement reads one InnoDB snapshot. On a pinned tab's
//! connection the read runs inside its open transaction and sees its
//! uncommitted rows; `KILL QUERY` on cancel ends only the statement, not the
//! transaction.

use std::sync::Arc;

use mysql_async::prelude::*;
use mysql_async::{Pool, Value};
use mystique_connector_api::{
    stream_cancelled, stream_column, ConnectorError, ConnectorResult, InterruptGuard, QueryControl,
    RowBatcher, RowSink, RowSource,
};
use mystique_sql::{qualify_table, quote_ident, SqlDialect};

use super::cancel::arm_kill;
use super::edits::mysql_params;
use super::encode::{mysql_type_label, value_json};
use super::query_failed;

pub(super) async fn stream_rows(
    pool: &Pool,
    conn: &mut mysql_async::Conn,
    source: &RowSource,
    control: &Arc<QueryControl>,
    sink: &mut dyn RowSink,
) -> ConnectorResult<()> {
    arm_kill(pool, conn.id(), control);
    let _guard = InterruptGuard::new(control);
    let id = conn.id();
    let read = match source {
        RowSource::Table {
            database,
            schema,
            table,
        } => {
            let keys = key_columns(conn, database.as_deref(), table).await?;
            let target = qualify_table(
                SqlDialect::Mysql,
                database.as_deref(),
                schema.as_deref(),
                table,
            );
            let order = if keys.is_empty() {
                String::new()
            } else {
                let list: Vec<String> = keys
                    .iter()
                    .map(|k| quote_ident(SqlDialect::Mysql, k))
                    .collect();
                format!(" ORDER BY {}", list.join(", "))
            };
            // Text protocol, like the table grid, so values encode the same.
            let sql = format!("SELECT * FROM {target}{order}");
            match conn.query_iter(sql.as_str()).await {
                Ok(mut result) => drain(&mut result, &keys, control, sink).await,
                Err(err) => Err(query_failed(err)),
            }
        }
        RowSource::Select { sql, params } => {
            match conn.exec_iter(sql.as_str(), mysql_params(params)).await {
                Ok(mut result) => drain(&mut result, &[], control, sink).await,
                Err(err) => Err(query_failed(err)),
            }
        }
    };
    control.clear_interrupt();
    if read.is_err() && !control.is_cancelled() {
        // The sink stopped early (the writer failed): end the statement on
        // the server rather than read the rest of the table to drain it.
        kill_query(pool, id).await;
    }
    read.map_err(|err| {
        if control.is_cancelled() {
            ConnectorError::Query("Query cancelled.".into())
        } else {
            err
        }
    })
}

async fn drain<P: Protocol>(
    result: &mut mysql_async::QueryResult<'_, '_, P>,
    keys: &[String],
    control: &Arc<QueryControl>,
    sink: &mut dyn RowSink,
) -> ConnectorResult<()> {
    let columns = result
        .columns_ref()
        .iter()
        .map(|c| {
            let name = c.name_str().into_owned();
            let mut column = stream_column(name.clone(), mysql_type_label(c.column_type()));
            column.pk = keys.contains(&name);
            column
        })
        .collect::<Vec<_>>();
    let width = columns.len();
    sink.columns(columns).await?;
    let mut batcher = RowBatcher::default();
    while let Some(row) = result.next().await.map_err(query_failed)? {
        let cells = (0..width)
            .map(|i| value_json(row.as_ref(i).cloned().unwrap_or(Value::NULL)))
            .collect();
        batcher.push(sink, control, cells).await?;
    }
    stream_cancelled(control)?;
    batcher.flush(sink).await
}

/// Primary-key columns in key order. `None` database means the current one.
async fn key_columns(
    conn: &mut mysql_async::Conn,
    database: Option<&str>,
    table: &str,
) -> ConnectorResult<Vec<String>> {
    conn.exec(
        r"SELECT COLUMN_NAME FROM information_schema.KEY_COLUMN_USAGE
          WHERE TABLE_SCHEMA = COALESCE(?, DATABASE()) AND TABLE_NAME = ?
            AND CONSTRAINT_NAME = 'PRIMARY'
          ORDER BY ORDINAL_POSITION",
        (database.map(str::to_string), table.to_string()),
    )
    .await
    .map_err(query_failed)
}

async fn kill_query(pool: &Pool, id: u32) {
    if let Ok(mut killer) = pool.get_conn().await {
        let _ = killer.query_drop(format!("KILL QUERY {id}")).await;
    }
}
