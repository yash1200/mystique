use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use mongodb::bson::doc;
use mongodb::Client;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities, PoolStatus,
};

pub struct MongoSession {
    client: Client,
    max_size: u32,
    #[allow(dead_code)]
    default_db: Option<String>,
    closed: AtomicBool,
}

impl MongoSession {
    pub fn new(client: Client, max_size: u32, default_db: Option<String>) -> Self {
        Self {
            client,
            max_size,
            default_db,
            closed: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl ConnectorSession for MongoSession {
    fn engine_id(&self) -> &str {
        "mongodb"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::mongodb_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        self.client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(|e| ConnectorError::Connect(e.to_string()))?;
        Ok(())
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        // Official driver does not expose live counters; report configured max.
        Some(PoolStatus {
            engine: "mongodb".into(),
            max_size: self.max_size,
            active: 0,
            idle: 0,
            size: self.max_size,
        })
    }

    async fn close(&self) -> ConnectorResult<()> {
        if self
            .closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.client.clone().shutdown().await;
        }
        Ok(())
    }
}
