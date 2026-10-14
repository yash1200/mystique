//! SQLite cancel is an [`rusqlite::InterruptHandle`] taken inside the blocking
//! closure, after the connection exists.

use std::time::{Duration, Instant};

use mystique_connector_api::{
    ConnectorError, ConnectorResult, QueryControl, INACTIVITY_TIMEOUT_MS,
};
use rusqlite::{Connection, ErrorCode};

use super::query_failed;

pub(super) fn arm(conn: &Connection, control: &QueryControl) {
    let handle = conn.get_interrupt_handle();
    control.set_interrupt(move || handle.interrupt());
}

pub(super) fn map_err(control: &QueryControl, err: rusqlite::Error) -> ConnectorError {
    if control.is_cancelled() || err.sqlite_error_code() == Some(ErrorCode::OperationInterrupted) {
        return ConnectorError::Query("Query cancelled.".into());
    }
    query_failed(err)
}

/// A late `interrupt()` sticks to the next statement on a pooled connection.
/// Clear the hook, then run a statement so a flag set after the real query
/// is consumed before the connection returns to the pool.
pub(super) struct Settle<'a> {
    pub conn: &'a Connection,
    pub control: &'a QueryControl,
}

impl Drop for Settle<'_> {
    fn drop(&mut self) {
        settle(self.conn, self.control);
    }
}

pub(super) fn settle(conn: &Connection, control: &QueryControl) {
    control.clear_interrupt();
    let _ = conn.query_row("SELECT 1", [], |_| Ok::<(), rusqlite::Error>(()));
}

pub(super) fn stalled(
    control: &QueryControl,
    saw_row: bool,
    last_row: Instant,
) -> ConnectorResult<()> {
    if control.is_cancelled() {
        return Err(ConnectorError::Query("Query cancelled.".into()));
    }
    if saw_row && last_row.elapsed() >= Duration::from_millis(INACTIVITY_TIMEOUT_MS) {
        return Err(ConnectorError::Timeout(format!(
            "No rows arrived for {INACTIVITY_TIMEOUT_MS} ms."
        )));
    }
    Ok(())
}
