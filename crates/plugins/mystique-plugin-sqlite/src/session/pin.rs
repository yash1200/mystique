//! One SQLite connection for a manual-commit tab.
//!
//! The file is opened beside the pool so a `BEGIN` cannot ride a pooled
//! connection back to the next checkout.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, PinnedSession, QueryBatchResult, QueryMode, ValidatedBatch,
};
use rusqlite::Connection;

use super::interrupt::map_err;
use super::query::{run_on_connection, RunOptions};
use super::query_failed;

pub struct SqlitePin {
    conn: Option<Connection>,
}

impl SqlitePin {
    pub fn open(path: &Path) -> ConnectorResult<Self> {
        let conn =
            Connection::open(path).map_err(|err| ConnectorError::Connect(err.to_string()))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|err| ConnectorError::Connect(err.to_string()))?;
        Ok(Self { conn: Some(conn) })
    }

    fn conn(&self) -> ConnectorResult<&Connection> {
        self.conn
            .as_ref()
            .ok_or_else(|| ConnectorError::Connect("pinned connection is closed".into()))
    }
}

#[async_trait]
impl PinnedSession for SqlitePin {
    async fn execute(&mut self, batch: &ValidatedBatch) -> ConnectorResult<QueryBatchResult> {
        let conn = self
            .conn
            .take()
            .ok_or_else(|| ConnectorError::Connect("pinned connection is closed".into()))?;
        let control = std::sync::Arc::clone(&batch.control);
        let statements = batch.statements.clone();
        let progress = batch.progress.clone();
        let lock_writes = batch.mode == QueryMode::Read || batch.explain_executes;
        // The first write opens a transaction (manual commit).
        let opts = RunOptions::for_batch(batch, lock_writes, !lock_writes);
        let joined = tokio::task::spawn_blocking(move || {
            let result = (|| {
                if lock_writes {
                    conn.execute_batch("PRAGMA query_only = ON")
                        .map_err(|err| map_err(&control, err))?;
                }
                let result = run_on_connection(&conn, &control, &statements, &progress, opts);
                if lock_writes {
                    let _ = conn.execute_batch("PRAGMA query_only = OFF");
                }
                result
            })();
            (conn, result)
        })
        .await
        .map_err(|err| ConnectorError::Internal(err.to_string()))?;
        self.conn = Some(joined.0);
        joined.1
    }

    async fn commit(&mut self) -> ConnectorResult<()> {
        if !self.in_transaction() {
            return Err(ConnectorError::Query(
                "No open transaction. Nothing was committed.".into(),
            ));
        }
        self.end_transaction(true).await
    }

    async fn rollback(&mut self) -> ConnectorResult<()> {
        self.end_transaction(false).await
    }

    fn in_transaction(&self) -> bool {
        self.conn().is_ok_and(|conn| !conn.is_autocommit())
    }
}

impl SqlitePin {
    async fn end_transaction(&mut self, commit: bool) -> ConnectorResult<()> {
        let conn = self
            .conn
            .take()
            .ok_or_else(|| ConnectorError::Connect("pinned connection is closed".into()))?;
        let joined = tokio::task::spawn_blocking(move || {
            let result = if conn.is_autocommit() {
                Ok(())
            } else {
                let sql = if commit { "COMMIT" } else { "ROLLBACK" };
                conn.execute_batch(sql).map_err(query_failed)
            };
            (conn, result)
        })
        .await
        .map_err(|err| ConnectorError::Internal(err.to_string()))?;
        self.conn = Some(joined.0);
        joined.1
    }
}

impl Drop for SqlitePin {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            if !conn.is_autocommit() {
                let _ = conn.execute_batch("ROLLBACK");
            }
        }
    }
}
