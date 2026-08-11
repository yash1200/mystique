use serde::{Deserialize, Serialize};

/// Lightweight engine card for Home / API catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDescriptor {
    pub id: String,
    pub display_name: String,
    pub family: crate::EngineFamily,
    pub query_language: String,
    pub available: bool,
    pub supports_pooling: bool,
    pub description: String,
    #[serde(default)]
    pub capabilities: Option<crate::EngineCapabilities>,
}

/// Live session handle returned to API clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: String,
    pub engine: String,
    pub display_name: String,
    pub capabilities: crate::EngineCapabilities,
    pub pool: Option<crate::PoolStatus>,
    pub opened_at: String,
}
