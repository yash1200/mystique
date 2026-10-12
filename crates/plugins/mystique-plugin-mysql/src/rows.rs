//! Row decode that returns [`mysql_async::FromRowError`] instead of panicking.
//!
//! `Queryable::query` / `exec` / `exec_map` call `FromRow::from_row`, which panics
//! when a column is NULL or the wrong width. Functional indexes have a NULL
//! `COLUMN_NAME`. That panic aborts the desktop app.

use mysql_async::prelude::{FromRow, Queryable};
use mysql_async::{FromRowError, Params};
use mystique_connector_api::{ConnectorError, ConnectorResult};

use crate::connector::pool::query_failed;

pub(crate) async fn collect_query<T>(
    conn: &mut mysql_async::Conn,
    sql: &str,
) -> ConnectorResult<Vec<T>>
where
    T: FromRow + Send + 'static,
{
    let rows = conn
        .query_iter(sql)
        .await
        .map_err(query_failed)?
        .try_collect_and_drop()
        .await
        .map_err(query_failed)?;
    flatten_rows(rows)
}

pub(crate) async fn collect_exec<T, P>(
    conn: &mut mysql_async::Conn,
    sql: &str,
    params: P,
) -> ConnectorResult<Vec<T>>
where
    T: FromRow + Send + 'static,
    P: Into<Params> + Send,
{
    let rows = conn
        .exec_iter(sql, params)
        .await
        .map_err(query_failed)?
        .try_collect_and_drop()
        .await
        .map_err(query_failed)?;
    flatten_rows(rows)
}

pub(crate) async fn query_first_opt<T>(
    conn: &mut mysql_async::Conn,
    sql: &str,
) -> ConnectorResult<Option<T>>
where
    T: FromRow + Send + 'static,
{
    let mut result = conn.query_iter(sql).await.map_err(query_failed)?;
    if result.is_empty() {
        result.drop_result().await.map_err(query_failed)?;
        return Ok(None);
    }
    let row = result.next().await.map_err(query_failed)?;
    result.drop_result().await.map_err(query_failed)?;
    match row {
        None => Ok(None),
        Some(row) => match T::from_row_opt(row) {
            Ok(value) => Ok(Some(value)),
            Err(err) => Err(decode_err(err)),
        },
    }
}

fn flatten_rows<T>(rows: Vec<Result<T, FromRowError>>) -> ConnectorResult<Vec<T>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        match row {
            Ok(value) => out.push(value),
            Err(err) => return Err(decode_err(err)),
        }
    }
    Ok(out)
}

fn decode_err(err: FromRowError) -> ConnectorError {
    ConnectorError::Query(format!("Could not decode a MySQL row: {err}"))
}
