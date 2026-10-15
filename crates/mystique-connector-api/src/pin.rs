//! One connection held for a query tab.
//!
//! Auto-commit checks a connection out of the pool and returns it. Manual
//! commit keeps this handle so `BEGIN`, `SET`, and temp tables survive until
//! commit, rollback, tab close, or the idle timeout.

use async_trait::async_trait;
use serde::Serialize;

use crate::batch::{QueryBatchResult, ValidatedBatch};
use crate::error::ConnectorResult;

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
}

/// What the editor shows for one tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PinStatus {
    pub pinned: bool,
    pub in_transaction: bool,
}

impl PinStatus {
    pub const fn idle() -> Self {
        Self {
            pinned: false,
            in_transaction: false,
        }
    }
}
