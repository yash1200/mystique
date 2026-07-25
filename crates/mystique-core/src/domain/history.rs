use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryHistoryItem {
    pub id: String,
    pub connection_id: Option<String>,
    pub engine: Option<String>,
    pub body: String,
    pub language: Option<String>,
    pub success: bool,
    pub row_count: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error_message: Option<String>,
    pub ran_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppendHistoryInput {
    pub connection_id: Option<String>,
    pub engine: Option<String>,
    pub body: String,
    pub language: Option<String>,
    pub success: bool,
    pub row_count: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error_message: Option<String>,
}
