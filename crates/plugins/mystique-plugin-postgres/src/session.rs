use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use deadpool_postgres::Pool;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities, PoolStatus,
};

pub struct PostgresSession {
    pool: Pool,
    max_size: u32,
    closed: AtomicBool,
}

impl PostgresSession {
    pub fn new(pool: Pool, max_size: u32) -> Self {
        Self {
            pool,
            max_size,
            closed: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl ConnectorSession for PostgresSession {
    fn engine_id(&self) -> &str {
        "postgres"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::postgres_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;
        client
            .simple_query("SELECT 1")
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        Ok(())
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        let status = self.pool.status();
        Some(PoolStatus {
            engine: "postgres".into(),
            max_size: self.max_size,
            active: status.size.saturating_sub(status.available) as u32,
            idle: status.available as u32,
            size: status.size as u32,
        })
    }

    async fn close(&self) -> ConnectorResult<()> {
        if self
            .closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.pool.close();
        }
        Ok(())
    }
}

