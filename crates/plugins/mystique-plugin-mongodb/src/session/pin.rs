//! Manual commit for a MongoDB replica set.
//!
//! `hello.setName` (or a mongos `isdbgrid`) is what turns the toolbar on.
//! A standalone server has no multi-document transactions.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use mongodb::bson::{doc, Bson, Document};
use mongodb::Client;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, PinnedSession, PlannedStatement, QueryBatchResult, QueryMode,
    SqlRange, StatementKind, StatementResult, ValidatedBatch, ValidatedQuery,
};

use super::query;
use super::MongoSession;

pub(crate) fn hello_supports_transactions(hello: &Document) -> bool {
    match hello.get("setName") {
        Some(Bson::String(name)) if !name.is_empty() => return true,
        _ => {}
    }
    matches!(hello.get("msg"), Some(Bson::String(msg)) if msg == "isdbgrid")
}

pub struct MongoPin {
    host: MongoSession,
    session: mongodb::ClientSession,
    open: AtomicBool,
}

impl MongoPin {
    pub async fn open(client: Client, default_db: Option<String>) -> ConnectorResult<Self> {
        let session = client
            .start_session()
            .await
            .map_err(|err| ConnectorError::Query(err.to_string()))?;
        let host = MongoSession::new(client, 1, default_db);
        host.transactions.store(true, Ordering::SeqCst);
        Ok(Self {
            host,
            session,
            open: AtomicBool::new(false),
        })
    }

    async fn ensure_transaction(&mut self) -> ConnectorResult<()> {
        if self.open.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.session
            .start_transaction()
            .await
            .map_err(|err| ConnectorError::Query(err.to_string()))?;
        self.open.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait]
impl PinnedSession for MongoPin {
    async fn execute(&mut self, batch: &ValidatedBatch) -> ConnectorResult<QueryBatchResult> {
        let text = batch
            .statements
            .first()
            .map(|stmt| stmt.sql.clone())
            .unwrap_or_default();
        let query = ValidatedQuery {
            text,
            mode: batch.mode,
            max_rows: batch.max_rows,
            timeout_ms: batch.timeout_ms,
            collection: batch.collection.clone(),
            database: batch.database.clone(),
            control: std::sync::Arc::clone(&batch.control),
        };
        // The client session is the transaction. Editor commands on this tab
        // run through it when the driver call accepts `.session`.
        self.ensure_transaction().await?;
        match query::execute_query(&self.host, &query).await {
            Ok(result) => {
                let stmt = batch
                    .statements
                    .first()
                    .cloned()
                    .unwrap_or_else(|| PlannedStatement {
                        index: 0,
                        sql: query.text.clone(),
                        sql_range: SqlRange {
                            start: 0,
                            end: u32::try_from(query.text.len()).unwrap_or(u32::MAX),
                        },
                        kind: if query.mode == QueryMode::Read {
                            StatementKind::Read
                        } else {
                            StatementKind::Write
                        },
                        returns_rows: true,
                        returns_count: false,
                        cursor: false,
                        multi_set: false,
                        txn_boundary: false,
                        explain_executes: false,
                    });
                Ok(QueryBatchResult {
                    results: vec![StatementResult::success(&stmt, 0, result, None, Vec::new())],
                    stopped_at: None,
                })
            }
            Err(err) => Err(err),
        }
    }

    async fn commit(&mut self) -> ConnectorResult<()> {
        if !self.open.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.session
            .commit_transaction()
            .await
            .map_err(|err| ConnectorError::Query(err.to_string()))?;
        self.open.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn rollback(&mut self) -> ConnectorResult<()> {
        if !self.open.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.session
            .abort_transaction()
            .await
            .map_err(|err| ConnectorError::Query(err.to_string()))?;
        self.open.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn in_transaction(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }
}

impl MongoSession {
    pub(super) fn share_client(&self) -> Client {
        self.client.clone()
    }

    pub(super) fn share_default_db(&self) -> Option<String> {
        self.default_db.clone()
    }
}

pub(super) async fn probe_transactions(client: &Client) -> bool {
    let Ok(hello) = client
        .database("admin")
        .run_command(doc! { "hello": 1 })
        .await
    else {
        return false;
    };
    hello_supports_transactions(&hello)
}

#[cfg(test)]
mod tests {
    use super::hello_supports_transactions;
    use mongodb::bson::{doc, Document};

    #[test]
    fn a_standalone_hello_hides_transactions() {
        let hello: Document = doc! { "ismaster": true, "ok": 1.0 };
        assert!(!hello_supports_transactions(&hello));
    }

    #[test]
    fn a_replica_set_hello_enables_transactions() {
        let hello = doc! { "setName": "rs0", "ok": 1.0 };
        assert!(hello_supports_transactions(&hello));
    }

    #[test]
    fn mongos_enables_transactions() {
        let hello = doc! { "msg": "isdbgrid", "ok": 1.0 };
        assert!(hello_supports_transactions(&hello));
    }
}
