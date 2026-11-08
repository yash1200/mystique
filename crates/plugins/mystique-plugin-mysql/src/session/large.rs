//! Re-read one clipped cell by primary key.

use mysql_async::prelude::*;
use mysql_async::Value;
use mystique_connector_api::{
    large_cell_select, page_cell, ConnectorError, ConnectorResult, FetchLargeValueInput,
    FetchLargeValueResult, InterruptGuard, SqlIdentStyle, SqlPlaceholder, SqlTextCast,
    FETCH_TIMEOUT_MS,
};

use super::cancel::{apply_timeouts, arm_kill, clear_timeouts};
use super::encode::value_json;
use super::query_failed;
use super::MysqlSession;

pub(super) async fn fetch_large_value(
    session: &MysqlSession,
    input: &FetchLargeValueInput,
) -> ConnectorResult<FetchLargeValueResult> {
    session.ensure_open()?;
    let select = large_cell_select(
        SqlIdentStyle::Mysql,
        SqlPlaceholder::Question,
        SqlTextCast::Char,
        &input.locator,
    )?;
    let mut conn = session.conn().await?;
    arm_kill(&session.pool, conn.id(), &input.control);
    let _guard = InterruptGuard::new(&input.control);
    apply_timeouts(&mut conn, FETCH_TIMEOUT_MS).await;
    let params: Vec<Value> = select
        .params
        .iter()
        .map(|value| Value::from(value.as_str()))
        .collect();
    let queried = conn.exec_iter(select.sql.as_str(), params).await;
    let cell = match queried {
        Err(err) => {
            input.control.clear_interrupt();
            let _ = conn.disconnect().await;
            return Err(if input.control.is_cancelled() {
                ConnectorError::Query("Query cancelled.".into())
            } else {
                query_failed(err)
            });
        }
        Ok(mut result_set) => {
            let next = result_set.next().await;
            input.control.clear_interrupt();
            clear_timeouts(&mut conn).await;
            match next {
                Ok(Some(row)) => row.as_ref(0).cloned().unwrap_or(Value::NULL),
                Ok(None) => {
                    return Err(ConnectorError::Query("No row matched that key.".into()));
                }
                Err(err) => {
                    return Err(if input.control.is_cancelled() {
                        ConnectorError::Query("Query cancelled.".into())
                    } else {
                        query_failed(err)
                    });
                }
            }
        }
    };
    Ok(page_cell(&value_json(cell), input.offset, input.limit))
}
