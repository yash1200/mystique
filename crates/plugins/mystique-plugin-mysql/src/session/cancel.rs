//! `KILL QUERY` from a second connection, plus both server timeout variables.
//!
//! The runtime id is `mysql` for MySQL, MariaDB, and TiDB. MariaDB ignores
//! `max_execution_time` and honors `max_statement_time` (seconds). Set both and
//! ignore the error from the one the server does not know. 0 on either means
//! no limit, which is what a [`NO_TIMEOUT`] run sets.

use mysql_async::prelude::*;
use mysql_async::Pool;
use mystique_connector_api::{QueryControl, NO_TIMEOUT};

use super::MysqlSession;

pub(super) fn statement_time_secs(timeout_ms: u64) -> u64 {
    if timeout_ms == NO_TIMEOUT {
        return 0;
    }
    timeout_ms.div_ceil(1_000).max(1)
}

pub(super) async fn apply_timeouts(conn: &mut mysql_async::Conn, timeout_ms: u64) {
    let millis = timeout_ms;
    let _ = conn
        .query_drop(format!("SET SESSION max_execution_time = {millis}"))
        .await;
    let secs = statement_time_secs(millis);
    let _ = conn
        .query_drop(format!("SET SESSION max_statement_time = {secs}"))
        .await;
}

pub(super) async fn clear_timeouts(conn: &mut mysql_async::Conn) {
    let _ = conn.query_drop("SET SESSION max_execution_time = 0").await;
    let _ = conn.query_drop("SET SESSION max_statement_time = 0").await;
}

/// Kill the running statement from a different pooled connection.
/// `KILL QUERY` on the busy connection cannot be sent.
pub(super) fn arm_kill(pool: &Pool, conn_id: u32, control: &QueryControl) {
    let pool = pool.clone();
    control.set_interrupt(move || {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        handle.spawn(async move {
            let Ok(mut killer) = pool.get_conn().await else {
                return;
            };
            let _ = killer.query_drop(format!("KILL QUERY {conn_id}")).await;
        });
    });
}

impl MysqlSession {
    pub(super) fn pool(&self) -> Pool {
        self.pool.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::statement_time_secs;

    #[test]
    fn mariadb_statement_time_is_whole_seconds() {
        assert_eq!(statement_time_secs(1_500), 2);
        assert_eq!(statement_time_secs(30_000), 30);
        assert_eq!(statement_time_secs(1), 1);
        assert_eq!(statement_time_secs(0), 0, "no timeout is no limit");
    }
}
