//! One pooled Postgres connection held for a manual-commit tab.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, InterruptGuard, PinnedSession, QueryBatchResult, QueryControl,
    ValidatedBatch,
};

use super::query::run_planned_batch;
use super::query_failed;
use super::{PgCancelTls, PostgresSession};

pub struct PostgresPin {
    client: Option<deadpool_postgres::Object>,
    cancel_tls: PgCancelTls,
    open: AtomicBool,
}

impl PostgresPin {
    pub fn new(client: deadpool_postgres::Object, cancel_tls: PgCancelTls) -> Self {
        Self {
            client: Some(client),
            cancel_tls,
            open: AtomicBool::new(false),
        }
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
        let Some(client) = self.client.as_ref() else {
            self.open.store(false, Ordering::SeqCst);
            return;
        };
        if let Some(open) = probe_transaction(client).await {
            self.open.store(open, Ordering::SeqCst);
        }
    }

    fn client(&self) -> ConnectorResult<&deadpool_postgres::Object> {
        self.client
            .as_ref()
            .ok_or_else(|| ConnectorError::Connect("pinned connection is closed".into()))
    }

    fn arm<'a>(
        client: &deadpool_postgres::Object,
        tls: &PgCancelTls,
        control: &'a QueryControl,
    ) -> InterruptGuard<'a> {
        let token = client.cancel_token();
        let tls = tls.clone();
        control.set_interrupt(move || {
            let Ok(handle) = tokio::runtime::Handle::try_current() else {
                return;
            };
            handle.spawn(async move {
                tls.cancel_query(token).await;
            });
        });
        InterruptGuard::new(control)
    }
}

#[async_trait]
impl PinnedSession for PostgresPin {
    async fn execute(&mut self, batch: &ValidatedBatch) -> ConnectorResult<QueryBatchResult> {
        let client = self.client()?;
        let _guard = Self::arm(client, &self.cancel_tls, &batch.control);
        let timeout = batch.timeout_ms.max(1);
        let _ = client
            .batch_execute(&format!("SET statement_timeout = {timeout}"))
            .await;
        let result = run_planned_batch(client, batch, false).await;
        let _ = client.batch_execute("RESET statement_timeout").await;
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

impl PostgresPin {
    async fn finish(&mut self, commit: bool) -> ConnectorResult<()> {
        self.refresh().await;
        if !self.open.load(Ordering::SeqCst) {
            return Ok(());
        }
        let client = self.client()?;
        let sql = if commit { "COMMIT" } else { "ROLLBACK" };
        match client.batch_execute(sql).await {
            Ok(()) => {
                self.open.store(false, Ordering::SeqCst);
                Ok(())
            }
            Err(err) if commit => {
                let _ = client.batch_execute("ROLLBACK").await;
                self.open.store(false, Ordering::SeqCst);
                Err(query_failed(err))
            }
            Err(err) => Err(query_failed(err)),
        }
    }
}

impl Drop for PostgresPin {
    fn drop(&mut self) {
        let Some(client) = self.client.take() else {
            return;
        };
        if !self.open.load(Ordering::SeqCst) {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        handle.spawn(async move {
            let _ = client.batch_execute("ROLLBACK").await;
        });
    }
}

async fn probe_transaction(client: &deadpool_postgres::Object) -> Option<bool> {
    let rows = client
        .query("SELECT txid_current_if_assigned() IS NOT NULL", &[])
        .await
        .ok()?;
    let row = rows.first()?;
    row.try_get(0).ok()
}

impl PostgresSession {
    pub(super) fn cancel_tls(&self) -> PgCancelTls {
        self.cancel_tls.clone()
    }
}
