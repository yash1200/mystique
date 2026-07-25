use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities, PoolStatus,
};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

pub struct SqliteSession {
    pool: Pool<SqliteConnectionManager>,
    max_size: u32,
    closed: AtomicBool,
}

impl SqliteSession {
    pub fn new(pool: Pool<SqliteConnectionManager>, max_size: u32) -> Self {
        Self {
            pool,
            max_size,
            closed: AtomicBool::new(false),
        }
    }

}

#[async_trait]
impl ConnectorSession for SqliteSession {
    fn engine_id(&self) -> &str {
        "sqlite"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::sqlite_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            conn.query_row("SELECT 1", [], |_| Ok(()))
                .map_err(|e| ConnectorError::Query(e.to_string()))
        })
        .await
        .map_err(|e| ConnectorError::Internal(e.to_string()))?
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        let state = self.pool.state();
        Some(PoolStatus {
            engine: "sqlite".into(),
            max_size: self.max_size,
            active: state.connections.saturating_sub(state.idle_connections),
            idle: state.idle_connections,
            size: state.connections,
        })
    }

    async fn close(&self) -> ConnectorResult<()> {
        // r2d2 pools drop when Arc count hits 0; mark closed for API.
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}
