use serde::{Deserialize, Serialize};

/// Named query saved for a connection (no groups).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: String,
    pub connection_id: String,
    pub name: String,
    pub body: String,
    pub language: Option<String>,
    pub sort_order: i64,
    pub favorite: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct SaveQueryInput {
    pub id: Option<String>,
    pub connection_id: String,
    pub name: String,
    pub body: String,
    pub language: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub sort_order: i64,
    /// `None` leaves the stored flag unchanged on update.
    #[serde(default)]
    pub favorite: Option<bool>,
}
