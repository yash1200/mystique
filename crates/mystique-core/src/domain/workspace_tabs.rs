use serde::{Deserialize, Serialize};

/// Persisted workspace tab (query canvas, table grid, or ER model).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTabRecord {
    pub id: String,
    pub connection_id: String,
    /// `query` | `table` | `model`
    pub kind: String,
    pub title: String,
    pub sort_order: i64,
    pub is_active: bool,
    /// Linked saved query when this is a saved query tab.
    #[serde(default)]
    pub saved_query_id: Option<String>,
    /// Draft SQL/MQL body for query tabs.
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub write_enabled: bool,
    /// Table id for kind === table.
    #[serde(default)]
    pub table_id: Option<String>,
    /// Database key for kind === model.
    #[serde(default)]
    pub model_database: Option<String>,
    /// Unsaved edits (query draft dirty, or table grid dirty).
    #[serde(default)]
    pub is_dirty: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWorkspaceTabInput {
    pub id: String,
    pub connection_id: String,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub saved_query_id: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub write_enabled: bool,
    #[serde(default)]
    pub table_id: Option<String>,
    #[serde(default)]
    pub model_database: Option<String>,
    #[serde(default)]
    pub is_dirty: bool,
}

/// Replace all open tabs for a connection (order = array order).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceWorkspaceTabsInput {
    pub connection_id: String,
    pub tabs: Vec<SaveWorkspaceTabInput>,
    /// Which tab id is active (optional; also set via is_active on items).
    #[serde(default)]
    pub active_tab_id: Option<String>,
}
