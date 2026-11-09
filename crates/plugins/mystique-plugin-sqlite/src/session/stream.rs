//! Streaming reads: SQLite steps one statement on a blocking thread and hands
//! row batches to the async side over a bounded channel. One statement reads
//! one snapshot. On a pinned tab's connection it runs inside the open
//! transaction and sees its uncommitted rows.

use std::sync::Arc;

use mystique_connector_api::{
    stream_column, ColumnInfo, ConnectorError, ConnectorResult, QueryControl, RowSink, RowSource,
    STREAM_BATCH_ROWS,
};
use mystique_sql::{quote_ident, SqlDialect};
use rusqlite::Connection;
use serde_json::Value as JsonValue;
use tokio::sync::mpsc;

use super::edits::json_binds;
use super::encode::value_ref_json;
use super::interrupt::{self, map_err};

pub(super) enum Msg {
    Columns(Vec<ColumnInfo>),
    Rows(Vec<Vec<JsonValue>>),
}

/// Batches the reader may run ahead of the sink.
const QUEUE: usize = 2;

pub(super) fn channel() -> (mpsc::Sender<Msg>, mpsc::Receiver<Msg>) {
    mpsc::channel(QUEUE)
}

/// Forward batches to the sink until the reader finishes. Dropping the
/// receiver on a sink error stops the reader at its next send.
pub(super) async fn pump(
    mut rx: mpsc::Receiver<Msg>,
    sink: &mut dyn RowSink,
) -> ConnectorResult<()> {
    while let Some(msg) = rx.recv().await {
        match msg {
            Msg::Columns(columns) => sink.columns(columns).await?,
            Msg::Rows(rows) => sink.rows(rows).await?,
        }
    }
    Ok(())
}

/// Runs on the blocking thread.
pub(super) fn read(
    conn: &Connection,
    source: &RowSource,
    control: &Arc<QueryControl>,
    tx: &mpsc::Sender<Msg>,
) -> ConnectorResult<()> {
    interrupt::arm(conn, control);
    let _settle = interrupt::Settle { conn, control };
    let (sql, params, keys) = match source {
        RowSource::Table { table, .. } => {
            let keys = key_columns(conn, table).map_err(|err| map_err(control, err))?;
            let target = quote_ident(SqlDialect::Sqlite, table);
            let order = if keys.is_empty() {
                String::new()
            } else {
                let list: Vec<String> = keys
                    .iter()
                    .map(|k| quote_ident(SqlDialect::Sqlite, k))
                    .collect();
                format!(" ORDER BY {}", list.join(", "))
            };
            (format!("SELECT * FROM {target}{order}"), Vec::new(), keys)
        }
        RowSource::Select { sql, params } => (sql.clone(), params.clone(), Vec::new()),
    };
    let mut stmt = conn.prepare(&sql).map_err(|err| map_err(control, err))?;
    let columns: Vec<ColumnInfo> = stmt
        .columns()
        .iter()
        .map(|c| {
            let mut column = stream_column(c.name(), c.decl_type().unwrap_or(""));
            column.pk = keys.iter().any(|k| k == c.name());
            column
        })
        .collect();
    let width = columns.len();
    send(tx, Msg::Columns(columns))?;
    let binds = json_binds(&params);
    let mut rows = stmt
        .query(rusqlite::params_from_iter(binds.iter()))
        .map_err(|err| map_err(control, err))?;
    let mut batch = Vec::with_capacity(STREAM_BATCH_ROWS);
    while let Some(row) = rows.next().map_err(|err| map_err(control, err))? {
        let mut cells = Vec::with_capacity(width);
        for i in 0..width {
            let value = row.get_ref(i).map_err(|err| map_err(control, err))?;
            cells.push(value_ref_json(value));
        }
        batch.push(cells);
        if batch.len() >= STREAM_BATCH_ROWS {
            if control.is_cancelled() {
                return Err(ConnectorError::Query("Query cancelled.".into()));
            }
            send(tx, Msg::Rows(std::mem::take(&mut batch)))?;
        }
    }
    if !batch.is_empty() {
        send(tx, Msg::Rows(batch))?;
    }
    Ok(())
}

fn send(tx: &mpsc::Sender<Msg>, msg: Msg) -> ConnectorResult<()> {
    tx.blocking_send(msg)
        .map_err(|_| ConnectorError::Internal("the export stopped reading".into()))
}

fn key_columns(conn: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT name FROM pragma_table_info(?1) WHERE pk > 0 ORDER BY pk")?;
    let names = stmt.query_map([table], |row| row.get::<_, String>(0))?;
    names.collect()
}
