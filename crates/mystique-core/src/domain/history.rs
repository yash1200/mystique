use serde::{Deserialize, Serialize};

/// Latest runs kept for one connection. Older rows are deleted on insert.
/// Rows with no connection are capped the same way.
pub const QUERY_HISTORY_CAP: i64 = 2_000;

/// Who ran the statement. The connection manager sets this from the session.
/// The query canvas does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum QueryHistorySource {
    #[default]
    Ui,
    Agent,
    Mcp,
}

impl QueryHistorySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ui => "ui",
            Self::Agent => "agent",
            Self::Mcp => "mcp",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "agent" => Self::Agent,
            "mcp" => Self::Mcp,
            _ => Self::Ui,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
    /// Catalog or database name. Never a connection URL.
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub affected_rows: Option<i64>,
    #[serde(default)]
    pub source: QueryHistorySource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
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
    /// Catalog name. A URI is dropped at insert.
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub affected_rows: Option<i64>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub source: QueryHistorySource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct QueryHistoryPage {
    pub items: Vec<QueryHistoryItem>,
    pub has_more: bool,
}

/// Keyset page. `cursor_ran_at` + `cursor_id` are the last row of the previous page.
#[derive(Debug, Clone)]
pub struct HistoryList {
    pub connection_id: Option<String>,
    pub limit: i64,
    pub search: Option<String>,
    pub cursor_ran_at: Option<String>,
    pub cursor_id: Option<String>,
}

/// Replace `scheme://user:password@host` passwords with `***`.
///
/// SQL text is kept. Connection-string userinfo is not. Bind parameters are
/// not a history column; only the statement text is stored.
pub fn redact_history_text(text: &str) -> String {
    crate::redact::redact_uri_passwords(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_removes_mongo_and_sql_userinfo_passwords() {
        let mongo = redact_history_text("mongodb://alice:secret-token@127.0.0.1:27017/app");
        assert_eq!(mongo, "mongodb://alice:***@127.0.0.1:27017/app");
        assert!(!mongo.contains("secret-token"));

        let srv = redact_history_text("mongodb+srv://user:pw@cluster.example/db");
        assert_eq!(srv, "mongodb+srv://user:***@cluster.example/db");

        let pg = redact_history_text("postgresql://app:s3cret@db.example:5432/app");
        assert!(!pg.contains("s3cret"));
        assert!(pg.contains("app:***@"));
    }

    #[test]
    fn redact_keeps_sql_and_uris_without_passwords() {
        assert_eq!(redact_history_text("SELECT 1"), "SELECT 1");
        assert_eq!(
            redact_history_text("mongodb://127.0.0.1:27017/app"),
            "mongodb://127.0.0.1:27017/app"
        );
        let quoted = redact_history_text("SELECT 'mongodb://alice:secret-token@h/db'");
        assert!(quoted.contains("SELECT"));
        assert!(quoted.contains("alice:***@"));
        assert!(!quoted.contains("secret-token"));
    }
}
