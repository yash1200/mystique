use serde::{Deserialize, Serialize};

/// Named query saved for a connection (no groups).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: String,
    pub connection_id: String,
    pub name: String,
    pub body: String,
    pub language: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveQueryInput {
    pub id: Option<String>,
    pub connection_id: String,
    pub name: String,
    pub body: String,
    pub language: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
}
