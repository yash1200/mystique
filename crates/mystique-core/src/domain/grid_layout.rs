use serde::{Deserialize, Serialize};

/// Frozen, hidden, and ordered columns for one table on one connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GridColumnLayoutRecord {
    pub connection_id: String,
    pub table_id: String,
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default)]
    pub frozen: Vec<String>,
}

/// Inverse SQL for one committed grid save.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridEditHistoryRecord {
    pub id: i64,
    pub connection_id: String,
    pub table_id: String,
    pub rollback_sql: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppendGridEditHistoryInput {
    pub connection_id: String,
    pub table_id: String,
    pub rollback_sql: Vec<String>,
}
