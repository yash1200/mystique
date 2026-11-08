//! Read mode lock: session read-only flag plus a read-only transaction.
//!
//! `START TRANSACTION READ ONLY` alone does not stop DDL (it commits the
//! transaction implicitly and runs), and does not stop DML after a `COMMIT`.
//! The session flag does: every later transaction in the session is read-only
//! (error 1792). The flag is reset before the connection goes back to the pool,
//! or the connection is closed.

use mysql_async::consts::StatusFlags;
use mysql_async::prelude::*;
use mystique_connector_api::{ConnectorError, ConnectorResult, QueryControl};

use super::cancel::clear_timeouts;
use super::query_failed;

/// How the server spells the session read-only switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadOnlyVar {
    /// MySQL 5.7.20+ and MariaDB 11.1+.
    TransactionReadOnly,
    /// Older MySQL and MariaDB before 11.1.
    TxReadOnly,
    /// TiDB accepts the syntax only as a no-op (`tidb_enable_noop_functions`),
    /// so it cannot lock a session.
    Unsupported,
}

impl ReadOnlyVar {
    /// Pick the variable from the product id and the `VERSION()` string.
    pub(crate) fn detect(engine: &str, version: &str) -> Self {
        let lower = version.to_ascii_lowercase();
        if engine.eq_ignore_ascii_case("tidb") || lower.contains("tidb") {
            return Self::Unsupported;
        }
        // Old MariaDB handshakes prefix the real version with `5.5.5-`.
        let text = lower.strip_prefix("5.5.5-").unwrap_or(&lower);
        let at_least = |want: (u32, u32, u32)| parse_version(text).is_some_and(|got| got >= want);
        let new_name = if lower.contains("mariadb") {
            at_least((11, 1, 0))
        } else {
            at_least((5, 7, 20))
        };
        if new_name {
            Self::TransactionReadOnly
        } else {
            Self::TxReadOnly
        }
    }

    fn name(self) -> Option<&'static str> {
        match self {
            Self::TransactionReadOnly => Some("transaction_read_only"),
            Self::TxReadOnly => Some("tx_read_only"),
            Self::Unsupported => None,
        }
    }
}

fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let head = text
        .split(|ch: char| ch != '.' && !ch.is_ascii_digit())
        .next()?;
    let mut parts = head.split('.').map(|part| part.parse::<u32>().ok());
    let major = parts.next()??;
    let minor = parts.next().flatten().unwrap_or(0);
    let patch = parts.next().flatten().unwrap_or(0);
    Some((major, minor, patch))
}

const UNSUPPORTED: &str = "This server (TiDB) cannot enforce a read-only session, so Read mode is not available. Use a database user without write privileges.";

/// Set the session flag, open a read-only transaction, and check both took.
/// On failure the caller must close the connection: it may hold the flag.
pub(super) async fn lock(
    conn: &mut mysql_async::Conn,
    var: ReadOnlyVar,
    control: &QueryControl,
) -> ConnectorResult<()> {
    let Some(name) = var.name() else {
        return Err(ConnectorError::Query(UNSUPPORTED.into()));
    };
    let failed = |err: mysql_async::Error| {
        if control.is_cancelled() {
            ConnectorError::Query("Query cancelled.".into())
        } else {
            query_failed(err)
        }
    };
    // The pool resets session state on return; keep that on for this connection.
    conn.reset_connection(true);
    conn.query_drop(format!("SET SESSION {name} = 1"))
        .await
        .map_err(failed)?;
    conn.query_drop("START TRANSACTION READ ONLY")
        .await
        .map_err(failed)?;
    ensure_locked(conn, var).await
}

/// The session flag is still on and a read-only transaction is still open.
/// Catches `SET SESSION transaction_read_only = 0`, `COMMIT`, and
/// `START TRANSACTION READ WRITE` between statements.
pub(super) async fn ensure_locked(
    conn: &mut mysql_async::Conn,
    var: ReadOnlyVar,
) -> ConnectorResult<()> {
    let Some(name) = var.name() else {
        return Err(ConnectorError::Query(UNSUPPORTED.into()));
    };
    // Cast so a numeric or string column both come back as text.
    let value = crate::rows::query_first_opt::<String>(
        conn,
        &format!("SELECT CASE WHEN @@{name} = 1 THEN 'on' ELSE 'off' END"),
    )
    .await?;
    // The status flags come from the end of that SELECT.
    let flags = conn.last_ok_packet().map(|ok| ok.status_flags());
    if is_locked(value.as_deref(), flags) {
        return Ok(());
    }
    Err(ConnectorError::Query(
        "The read-only transaction ended before every statement ran. Later statements were not executed.".into(),
    ))
}

fn is_locked(value: Option<&str>, flags: Option<StatusFlags>) -> bool {
    let want = StatusFlags::SERVER_STATUS_IN_TRANS | StatusFlags::SERVER_STATUS_IN_TRANS_READONLY;
    value == Some("on") && flags.is_some_and(|flags| flags.contains(want))
}

/// Roll back, clear the session flag, and drop the timeouts. If either reset
/// fails the connection is closed, so a read-only session never reaches a
/// later Write-mode checkout.
pub(super) async fn unlock<T>(
    mut conn: mysql_async::Conn,
    var: ReadOnlyVar,
    result: ConnectorResult<T>,
) -> ConnectorResult<T> {
    let mut reset = conn.query_drop("ROLLBACK").await;
    // DEFAULT, not 0: a server that is read-only by configuration stays so.
    if let (Ok(()), Some(name)) = (&reset, var.name()) {
        reset = conn
            .query_drop(format!("SET SESSION {name} = DEFAULT"))
            .await;
    }
    match reset {
        Ok(()) => {
            clear_timeouts(&mut conn).await;
            result
        }
        Err(err) => {
            let _ = conn.disconnect().await;
            result.and(Err(query_failed(err)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_locked, parse_version, ReadOnlyVar};
    use mysql_async::consts::StatusFlags;

    #[test]
    fn mysql_8_uses_transaction_read_only() {
        assert_eq!(
            ReadOnlyVar::detect("mysql", "8.4.3"),
            ReadOnlyVar::TransactionReadOnly
        );
        assert_eq!(
            ReadOnlyVar::detect("mysql", "26.7.0"),
            ReadOnlyVar::TransactionReadOnly
        );
        assert_eq!(
            ReadOnlyVar::detect("mysql", "5.7.19-log"),
            ReadOnlyVar::TxReadOnly
        );
    }

    #[test]
    fn mariadb_before_11_1_uses_tx_read_only() {
        assert_eq!(
            ReadOnlyVar::detect("mariadb", "10.11.6-MariaDB-1:10.11.6+maria~ubu2204"),
            ReadOnlyVar::TxReadOnly
        );
        assert_eq!(
            ReadOnlyVar::detect("mysql", "5.5.5-10.6.16-MariaDB"),
            ReadOnlyVar::TxReadOnly
        );
        assert_eq!(
            ReadOnlyVar::detect("mariadb", "11.4.2-MariaDB"),
            ReadOnlyVar::TransactionReadOnly
        );
    }

    #[test]
    fn tidb_cannot_lock_even_under_a_mysql_profile() {
        assert_eq!(
            ReadOnlyVar::detect("mysql", "8.0.11-TiDB-v7.5.1"),
            ReadOnlyVar::Unsupported
        );
        assert_eq!(
            ReadOnlyVar::detect("tidb", "8.0.11"),
            ReadOnlyVar::Unsupported
        );
    }

    #[test]
    fn unparseable_version_falls_back_to_the_old_name() {
        assert_eq!(parse_version("garbage"), None);
        assert_eq!(
            ReadOnlyVar::detect("mysql", "garbage"),
            ReadOnlyVar::TxReadOnly
        );
    }

    #[test]
    fn locked_needs_the_flag_and_an_open_read_only_transaction() {
        let open = StatusFlags::SERVER_STATUS_IN_TRANS
            | StatusFlags::SERVER_STATUS_IN_TRANS_READONLY
            | StatusFlags::SERVER_STATUS_AUTOCOMMIT;
        assert!(is_locked(Some("on"), Some(open)));
        assert!(!is_locked(Some("off"), Some(open)));
        // COMMIT ended the transaction.
        assert!(!is_locked(
            Some("on"),
            Some(StatusFlags::SERVER_STATUS_AUTOCOMMIT)
        ));
        // START TRANSACTION READ WRITE.
        assert!(!is_locked(
            Some("on"),
            Some(StatusFlags::SERVER_STATUS_IN_TRANS)
        ));
        assert!(!is_locked(Some("on"), None));
    }
}
