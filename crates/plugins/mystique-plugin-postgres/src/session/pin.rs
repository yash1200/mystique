//! One pooled Postgres connection held for a manual-commit tab.

use std::sync::Arc;

use async_trait::async_trait;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, InterruptGuard, PinnedSession, QueryBatchResult, QueryControl,
    RowSink, RowSource, ValidatedBatch,
};

use super::query::{run_planned_batch, transaction_state, TxnState};
use super::query_failed;
use super::stream::{stream_rows, Wrap};
use super::{NoticeHub, PgCancelTls, PostgresSession};

pub struct PostgresPin {
    client: Option<deadpool_postgres::Object>,
    cancel_tls: PgCancelTls,
    notices: NoticeHub,
    state: TxnState,
}

impl PostgresPin {
    pub(crate) fn new(
        client: deadpool_postgres::Object,
        cancel_tls: PgCancelTls,
        notices: NoticeHub,
    ) -> Self {
        Self {
            client: Some(client),
            cancel_tls,
            notices,
            state: TxnState::Idle,
        }
    }

    async fn refresh(&mut self) {
        let Some(client) = self.client.as_ref() else {
            self.state = TxnState::Idle;
            return;
        };
        if let Some(state) = transaction_state(client).await {
            self.state = state;
        } else if client.is_closed() {
            self.state = TxnState::Idle;
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
        let timeout = batch.timeout_ms; // 0 turns statement_timeout off.
        let _ = client
            .batch_execute(&format!("SET statement_timeout = {timeout}"))
            .await;
        // The first write opens a transaction (manual commit). Inside one, a
        // cursor is declared directly; its own BEGIN … COMMIT would commit
        // the open writes.
        let in_txn = self.state != TxnState::Idle;
        let notices = self.notices.capture(client).await;
        let result = run_planned_batch(client, batch, false, in_txn, true, notices.as_ref()).await;
        drop(notices);
        let _ = client.batch_execute("RESET statement_timeout").await;
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
        self.state != TxnState::Idle
    }

    async fn stream_rows(
        &mut self,
        source: &RowSource,
        control: &Arc<QueryControl>,
        sink: &mut dyn RowSink,
    ) -> ConnectorResult<()> {
        self.refresh().await;
        if self.state == TxnState::Failed {
            return Err(ConnectorError::Query(
                "This tab's transaction has failed. Roll it back before exporting from it.".into(),
            ));
        }
        let wrap = if self.state == TxnState::Open {
            Wrap::Savepoint
        } else {
            Wrap::Snapshot
        };
        let client = self.client()?;
        let _guard = Self::arm(client, &self.cancel_tls, control);
        let result = stream_rows(client, wrap, source, control, sink).await;
        self.refresh().await;
        result
    }
}

impl PostgresPin {
    async fn finish(&mut self, commit: bool) -> ConnectorResult<()> {
        self.refresh().await;
        let state = self.state;
        if state == TxnState::Idle {
            return if commit {
                Err(ConnectorError::Query(
                    "No open transaction. Nothing was committed.".into(),
                ))
            } else {
                Ok(())
            };
        }
        let client = self.client()?;
        // COMMIT of a failed transaction rolls back without an error. Say so.
        let commit_sql = commit && state == TxnState::Open;
        let ended = client
            .batch_execute(if commit_sql { "COMMIT" } else { "ROLLBACK" })
            .await;
        if ended.is_err() && commit_sql {
            let _ = client.batch_execute("ROLLBACK").await;
        }
        self.refresh().await;
        ended.map_err(query_failed)?;
        if commit && !commit_sql {
            return Err(ConnectorError::Query(
                "A statement in the transaction failed, so it was rolled back. Nothing was committed."
                    .into(),
            ));
        }
        Ok(())
    }
}

impl Drop for PostgresPin {
    fn drop(&mut self) {
        let Some(client) = self.client.take() else {
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            // No runtime to reset it on. Close it rather than pool it.
            drop(deadpool_postgres::Object::take(client));
            return;
        };
        handle.spawn(async move {
            if reset_session(&client).await.is_err() {
                drop(deadpool_postgres::Object::take(client));
            }
        });
    }
}

/// Leave nothing for the next pool user: an open transaction, `SET`, role,
/// temp tables, advisory locks, prepared statements. DISCARD ALL refuses to run
/// inside a transaction block, so on failure roll back and run it again.
async fn reset_session(client: &deadpool_postgres::Object) -> Result<(), tokio_postgres::Error> {
    if client.batch_execute("DISCARD ALL").await.is_err() {
        client.batch_execute("ROLLBACK").await?;
        client.batch_execute("DISCARD ALL").await?;
    }
    // DISCARD ALL also drops the statements the driver cached for type lookups.
    client.clear_type_cache();
    client.statement_cache.clear();
    Ok(())
}

impl PostgresSession {
    pub(super) fn cancel_tls(&self) -> PgCancelTls {
        self.cancel_tls.clone()
    }
}
