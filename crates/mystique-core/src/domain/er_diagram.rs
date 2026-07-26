use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErDiagram {
    pub id: String,
    pub name: String,
    /// sql | nosql | redis
    pub family: String,
    /// Full diagram JSON (nodes, edges, …)
    pub body: JsonValue,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveErDiagramInput {
    pub id: Option<String>,
    pub name: String,
    pub family: String,
    pub body: JsonValue,
}
