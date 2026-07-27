use async_trait::async_trait;

use crate::capabilities::EngineCapabilities;
use crate::config::ConnectionConfig;
use crate::error::ConnectorResult;
use crate::form::ConnectionFormSchema;
use crate::pool::{PoolStatus, TestResult};
use crate::query::{QueryResult, ValidatedQuery};
use crate::types::EngineDescriptor;
use crate::EngineFamily;

/// Factory for one database engine (postgres, sqlite, mongodb, …).
///
/// Implement this in a plugin crate and register it once.
#[async_trait]
pub trait DatabaseConnector: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn family(&self) -> EngineFamily;
    fn description(&self) -> &'static str;

    fn capabilities(&self) -> EngineCapabilities;
    fn connection_form_schema(&self) -> ConnectionFormSchema;

    fn descriptor(&self) -> EngineDescriptor {
        let caps = self.capabilities();
        EngineDescriptor {
            id: self.id().into(),
            display_name: self.display_name().into(),
            family: self.family(),
            query_language: caps.query_language,
            available: true,
            supports_pooling: caps.supports_pooling,
            description: self.description().into(),
        }
    }

    /// Cheap connectivity check (may open a short-lived connection).
    async fn test_connection(&self, config: &ConnectionConfig) -> ConnectorResult<TestResult>;

    /// Open a pooled session for ongoing work.
    async fn connect(&self, config: &ConnectionConfig) -> ConnectorResult<Box<dyn ConnectorSession>>;
}

/// Live session backed by an engine-specific connection pool.
#[async_trait]
pub trait ConnectorSession: Send + Sync {
    fn engine_id(&self) -> &str;
    fn capabilities(&self) -> EngineCapabilities;

    /// Health check using a pooled connection.
    async fn ping(&self) -> ConnectorResult<()>;

    /// Current pool snapshot (if the engine pools).
    fn pool_status(&self) -> Option<PoolStatus>;

    /// Drain the pool and release resources.
    async fn close(&self) -> ConnectorResult<()>;

    /// Run a policy-validated query. Default: not supported.
    async fn execute_query(&self, _query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        Err(crate::ConnectorError::NotSupported(format!(
            "execute_query not implemented for engine `{}`",
            self.engine_id()
        )))
    }
}
