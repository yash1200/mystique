//! One connection held for a query tab.
//!
//! Auto-commit checks a connection out of the pool and returns it. Manual
//! commit keeps this handle so `BEGIN`, `SET`, and temp tables survive until
//! commit, rollback, tab close, or the idle timeout.
//!
//! With auto-commit off, the first write opens a transaction on every engine
//! (see [`opens_transaction`]). Reads before it run without one.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;

use crate::batch::{PlannedStatement, QueryBatchResult, StatementKind, ValidatedBatch};
use crate::cancel::QueryControl;
use crate::error::ConnectorResult;
use crate::stream::{RowSink, RowSource};
use crate::SqlDialect;

/// A connection that outlives a single statement.
///
/// `execute` runs a planned batch on that connection. `commit` and `rollback`
/// end the open transaction and leave the connection pinned.
#[async_trait]
pub trait PinnedSession: Send {
    async fn execute(&mut self, batch: &ValidatedBatch) -> ConnectorResult<QueryBatchResult>;
    async fn commit(&mut self) -> ConnectorResult<()>;
    async fn rollback(&mut self) -> ConnectorResult<()>;
    fn in_transaction(&self) -> bool;

    /// Stream rows on this connection, inside the open transaction, so an
    /// export or a large-value read sees the tab's uncommitted changes.
    ///
    /// Must leave the transaction as it was: a failed or cancelled read does
    /// not abort or end it (Postgres reads under a savepoint). Default:
    /// not supported, and callers say so rather than read outside it.
    async fn stream_rows(
        &mut self,
        _source: &RowSource,
        _control: &Arc<QueryControl>,
        _sink: &mut dyn RowSink,
    ) -> ConnectorResult<()> {
        Err(crate::ConnectorError::NotSupported(
            "Reading through this tab's open transaction is not supported for this engine.".into(),
        ))
    }
}

/// What the editor shows for one tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PinStatus {
    pub pinned: bool,
    pub in_transaction: bool,
    /// An open transaction was rolled back: on unpin, or without the user
    /// asking (idle timeout, disconnect). Nothing in it was committed.
    pub rolled_back: bool,
}

impl PinStatus {
    pub const fn idle() -> Self {
        Self {
            pinned: false,
            in_transaction: false,
            rolled_back: false,
        }
    }

    pub const fn pinned(in_transaction: bool) -> Self {
        Self {
            pinned: true,
            in_transaction,
            rolled_back: false,
        }
    }
}

/// Manual commit: open a transaction before `stmt` when none is open.
///
/// Writes do. Reads, `SET`, and the user's own `BEGIN` / `COMMIT` do not, so
/// a typed `BEGIN` never runs inside one we opened. Statements the engine
/// refuses inside a transaction block run as typed.
pub fn opens_transaction(stmt: &PlannedStatement, dialect: SqlDialect) -> bool {
    if stmt.txn_boundary {
        return false;
    }
    if !matches!(stmt.kind, StatementKind::Write | StatementKind::Destructive) {
        return false;
    }
    !refused_in_transaction(&stmt.sql, dialect)
}

fn refused_in_transaction(sql: &str, dialect: SqlDialect) -> bool {
    let lower = skip_leading_comments(sql).to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .collect();
    let first = words.first().copied().unwrap_or("");
    let second = words.get(1).copied().unwrap_or("");
    match dialect {
        SqlDialect::Postgres => {
            first == "vacuum"
                || matches!(
                    (first, second),
                    (
                        "create" | "drop",
                        "database" | "tablespace" | "subscription"
                    ) | ("alter", "system")
                        | ("reindex", "system" | "database")
                )
                || (matches!(first, "create" | "drop" | "reindex" | "alter")
                    && words.contains(&"concurrently"))
        }
        SqlDialect::Sqlite => matches!(first, "vacuum" | "attach" | "detach"),
        SqlDialect::Mysql | SqlDialect::Generic => false,
    }
}

fn skip_leading_comments(sql: &str) -> &str {
    let mut rest = sql.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("--") {
            rest = after
                .split_once('\n')
                .map_or("", |(_, tail)| tail)
                .trim_start();
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after
                .split_once("*/")
                .map_or("", |(_, tail)| tail)
                .trim_start();
        } else {
            return rest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{gate_sql_batch_in, QueryMode};

    fn opens(sql: &str, dialect: SqlDialect) -> Vec<bool> {
        gate_sql_batch_in(sql, dialect, QueryMode::Write, true, &[])
            .expect("plan")
            .statements
            .iter()
            .map(|stmt| opens_transaction(stmt, dialect))
            .collect()
    }

    #[test]
    fn writes_open_a_transaction_and_reads_do_not() {
        for dialect in [SqlDialect::Postgres, SqlDialect::Mysql, SqlDialect::Sqlite] {
            assert_eq!(
                opens("SELECT 1; INSERT INTO t VALUES (1); DELETE FROM t", dialect),
                vec![false, true, true],
                "{dialect:?}"
            );
            assert_eq!(opens("DROP TABLE t", dialect), vec![true], "{dialect:?}");
        }
    }

    #[test]
    fn a_typed_begin_or_commit_is_never_doubled() {
        assert_eq!(
            opens(
                "BEGIN; INSERT INTO t VALUES (1); COMMIT",
                SqlDialect::Postgres
            ),
            vec![false, true, false]
        );
        assert_eq!(
            opens("START TRANSACTION; UPDATE t SET a = 1", SqlDialect::Mysql),
            vec![false, true]
        );
        assert_eq!(
            opens("SET search_path = public", SqlDialect::Postgres),
            vec![false]
        );
    }

    #[test]
    fn statements_refused_in_a_transaction_run_as_typed() {
        for sql in [
            "VACUUM t",
            "-- tidy\nVACUUM",
            "CREATE INDEX CONCURRENTLY i ON t (a)",
            "DROP INDEX CONCURRENTLY i",
            "CREATE DATABASE d",
            "ALTER SYSTEM SET work_mem = '64MB'",
        ] {
            assert_eq!(opens(sql, SqlDialect::Postgres), vec![false], "{sql}");
        }
        assert_eq!(
            opens("CREATE INDEX i ON t (a)", SqlDialect::Postgres),
            vec![true]
        );
        assert_eq!(opens("VACUUM", SqlDialect::Sqlite), vec![false]);
        assert_eq!(opens("ATTACH 'x.db' AS x", SqlDialect::Sqlite), vec![false]);
    }
}
