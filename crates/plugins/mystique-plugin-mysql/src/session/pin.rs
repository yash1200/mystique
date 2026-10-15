//! One pooled MySQL connection held for a manual-commit tab.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use mysql_async::prelude::Queryable;
use mysql_async::Pool;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, InterruptGuard, PinnedSession, QueryBatchResult,
    ValidatedBatch,
};

use super::cancel::{self, arm_kill};
use super::query::run_planned_batch;
use super::query_failed;

pub struct MysqlPin {
    pool: Pool,
    conn: Option<mysql_async::Conn>,
    open: AtomicBool,
}

impl MysqlPin {
    pub fn new(pool: Pool, conn: mysql_async::Conn) -> Self {
        Self {
            pool,
            conn: Some(conn),
            open: AtomicBool::new(false),
        }
    }

    fn conn_mut(&mut self) -> ConnectorResult<&mut mysql_async::Conn> {
        self.conn
            .as_mut()
            .ok_or_else(|| ConnectorError::Connect("pinned connection is closed".into()))
    }

    fn note(&self, batch: &ValidatedBatch) {
        for stmt in &batch.statements {
            if !stmt.txn_boundary {
                continue;
            }
            let head = stmt
                .sql
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            let open = head != "commit" && head != "rollback";
            self.open.store(open, Ordering::SeqCst);
        }
    }

    async fn refresh(&mut self) {
        let Some(conn) = self.conn.as_mut() else {
            self.open.store(false, Ordering::SeqCst);
            return;
        };
        if let Some(open) = probe_transaction(conn).await {
            self.open.store(open, Ordering::SeqCst);
        }
    }

    async fn finish(&mut self, commit: bool) -> ConnectorResult<()> {
        self.refresh().await;
        if !self.open.load(Ordering::SeqCst) {
            return Ok(());
        }
        let conn = self.conn_mut()?;
        let sql = if commit { "COMMIT" } else { "ROLLBACK" };
        conn.query_drop(sql).await.map_err(query_failed)?;
        self.open.store(false, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait]
impl PinnedSession for MysqlPin {
    async fn execute(&mut self, batch: &ValidatedBatch) -> ConnectorResult<QueryBatchResult> {
        let pool = self.pool.clone();
        let conn = self.conn_mut()?;
        arm_kill(&pool, conn.id(), &batch.control);
        let _guard = InterruptGuard::new(&batch.control);
        cancel::apply_timeouts(conn, batch.timeout_ms).await;
        let result = run_planned_batch(conn, batch, false).await;
        batch.control.clear_interrupt();
        cancel::clear_timeouts(conn).await;
        self.note(batch);
        self.refresh().await;
        result
    }

    async fn commit(&mut self) -> ConnectorResult<()> {
        self.finish(true).await
    }

    async fn rollback(&mut self) -> ConnectorResult<()> {
        self.finish(false).await
    }

    fn in_transaction(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }
}

impl Drop for MysqlPin {
    fn drop(&mut self) {
        let Some(mut conn) = self.conn.take() else {
            return;
        };
        if !self.open.load(Ordering::SeqCst) {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        handle.spawn(async move {
            let _ = conn.query_drop("ROLLBACK").await;
        });
    }
}

async fn probe_transaction(conn: &mut mysql_async::Conn) -> Option<bool> {
    let value = crate::rows::query_first_opt::<i64>(
        conn,
        "SELECT COUNT(*) FROM information_schema.innodb_trx WHERE trx_mysql_thread_id = CONNECTION_ID()",
    )
    .await
    .ok()?;
    Some(value.unwrap_or(0) > 0)
}
