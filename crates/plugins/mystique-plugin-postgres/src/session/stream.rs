//! Streaming reads: a whole table through one cursor, or one bound statement.
//!
//! A table is read with `DECLARE … NO SCROLL CURSOR` and `FETCH n`, so the
//! server holds the result and the client holds one batch. On a pooled
//! connection that runs in a `REPEATABLE READ READ ONLY` transaction: one
//! snapshot for the whole export. Inside a pinned tab's open transaction it
//! runs under a savepoint, so the export sees the tab's uncommitted rows and
//! a failure or cancel leaves the tab's transaction as it was.
//!
//! Everything runs on the one connection given, so a pinned tab's
//! uncommitted DDL is visible too.

use std::sync::Arc;

use futures_util::StreamExt;
use mystique_connector_api::{
    stream_cancelled, stream_column, ConnectorResult, QueryControl, RowBatcher, RowSink, RowSource,
    STREAM_BATCH_ROWS,
};
use mystique_sql::{qualify_table, quote_ident, SqlDialect};
use tokio_postgres::types::ToSql;
use tokio_postgres::Client;

use super::encode::cell_json;
use super::query::q_binds;
use super::{pg_col, query_failed};

const EXPORT_CURSOR: &str = "mystique_export";
const SAVEPOINT: &str = "mystique_read";

/// How the read is wrapped.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Wrap {
    /// A pooled connection with no transaction: open a read-only snapshot.
    Snapshot,
    /// Inside the user's open transaction: a savepoint, released after.
    Savepoint,
}

pub(super) async fn stream_rows(
    client: &Client,
    wrap: Wrap,
    source: &RowSource,
    control: &Arc<QueryControl>,
    sink: &mut dyn RowSink,
) -> ConnectorResult<()> {
    let (begin, commit, abort) = match wrap {
        Wrap::Snapshot => (
            "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY".to_string(),
            "COMMIT".to_string(),
            "ROLLBACK".to_string(),
        ),
        Wrap::Savepoint => (
            format!("SAVEPOINT {SAVEPOINT}"),
            format!("RELEASE SAVEPOINT {SAVEPOINT}"),
            format!("ROLLBACK TO SAVEPOINT {SAVEPOINT}; RELEASE SAVEPOINT {SAVEPOINT}"),
        ),
    };
    client.batch_execute(&begin).await.map_err(query_failed)?;
    let read = match source {
        RowSource::Table {
            database,
            schema,
            table,
        } => {
            let target = qualify_table(
                SqlDialect::Postgres,
                database.as_deref(),
                schema.as_deref(),
                table,
            );
            read_table(client, &target, control, sink).await
        }
        RowSource::Select { sql, params } => read_bound(client, sql, params, control, sink).await,
    };
    let end = if read.is_ok() { &commit } else { &abort };
    match (read, client.batch_execute(end).await) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(err), _) => Err(err),
        (Ok(()), Err(err)) => Err(query_failed(err)),
    }
}

/// Primary-key columns in key order, or none. Read in the same transaction.
async fn key_columns(client: &Client, target: &str) -> ConnectorResult<Vec<String>> {
    let rows = client
        .query(
            r#"
            SELECT a.attname
            FROM pg_catalog.pg_index i
            CROSS JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS k(attnum, ord)
            JOIN pg_catalog.pg_attribute a
              ON a.attrelid = i.indrelid AND a.attnum = k.attnum
            WHERE i.indrelid = to_regclass($1)
              AND i.indisprimary
            ORDER BY k.ord
            "#,
            &[&target],
        )
        .await
        .map_err(query_failed)?;
    rows.iter().map(|row| pg_col::<String>(row, 0)).collect()
}

/// The table in key order when it has a key; storage order otherwise (no
/// sort of a keyless table).
async fn read_table(
    client: &Client,
    target: &str,
    control: &Arc<QueryControl>,
    sink: &mut dyn RowSink,
) -> ConnectorResult<()> {
    let keys = key_columns(client, target).await?;
    let order = if keys.is_empty() {
        String::new()
    } else {
        let list: Vec<String> = keys
            .iter()
            .map(|k| quote_ident(SqlDialect::Postgres, k))
            .collect();
        format!(" ORDER BY {}", list.join(", "))
    };
    client
        .batch_execute(&format!(
            "DECLARE {EXPORT_CURSOR} NO SCROLL CURSOR FOR SELECT * FROM {target}{order}"
        ))
        .await
        .map_err(query_failed)?;
    let fetch = client
        .prepare(&format!("FETCH {STREAM_BATCH_ROWS} FROM {EXPORT_CURSOR}"))
        .await
        .map_err(query_failed)?;
    let columns = fetch
        .columns()
        .iter()
        .map(|c| {
            let mut column = stream_column(c.name(), c.type_().name());
            column.pk = keys.iter().any(|k| k == c.name());
            column
        })
        .collect();
    sink.columns(columns).await?;
    loop {
        stream_cancelled(control)?;
        let rows = client.query(&fetch, &[]).await.map_err(query_failed)?;
        let done = rows.len() < STREAM_BATCH_ROWS;
        if !rows.is_empty() {
            let batch = rows
                .iter()
                .map(|row| (0..row.len()).map(|i| cell_json(row, i)).collect())
                .collect();
            drop(rows);
            sink.rows(batch).await?;
        }
        if done {
            break;
        }
    }
    client
        .batch_execute(&format!("CLOSE {EXPORT_CURSOR}"))
        .await
        .map_err(query_failed)
}

/// One bound statement, streamed (no cursor: DECLARE takes no parameters).
/// `$n` is sent as text and typed by the server, so `"id" = $1` uses the
/// key index whatever the key type.
async fn read_bound(
    client: &Client,
    sql: &str,
    params: &[serde_json::Value],
    control: &Arc<QueryControl>,
    sink: &mut dyn RowSink,
) -> ConnectorResult<()> {
    let binds = q_binds(params);
    let refs: Vec<&(dyn ToSql + Sync)> = binds.iter().map(|bind| bind as _).collect();
    let stream = client.query_raw(sql, refs).await.map_err(query_failed)?;
    let mut stream = std::pin::pin!(stream);
    let mut batcher = RowBatcher::default();
    let mut sent_columns = false;
    while let Some(row) = stream.next().await {
        let row = row.map_err(query_failed)?;
        if !sent_columns {
            let columns = row
                .columns()
                .iter()
                .map(|c| stream_column(c.name(), c.type_().name()))
                .collect();
            sink.columns(columns).await?;
            sent_columns = true;
        }
        let cells = (0..row.len()).map(|i| cell_json(&row, i)).collect();
        batcher.push(sink, control, cells).await?;
    }
    batcher.flush(sink).await
}
