use mystique_connector_api::HARD_MAX_ROWS;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

pub const AGENT_MAX_ROWS: u32 = 2_000;
pub const AGENT_DEFAULT_ROWS: u32 = 200;

/// Cap passed to the driver. Postgres fetches only this many rows from the server.
pub fn clamp_agent_max_rows(max_rows: Option<u32>) -> u32 {
    max_rows
        .unwrap_or(AGENT_DEFAULT_ROWS)
        .clamp(1, AGENT_MAX_ROWS)
}

const _: () = assert!(
    AGENT_MAX_ROWS < HARD_MAX_ROWS,
    "agent row cap must stay below the driver fetch ceiling"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_row_cap_clamps_zero_and_huge() {
        assert_eq!(clamp_agent_max_rows(None), AGENT_DEFAULT_ROWS);
        assert_eq!(clamp_agent_max_rows(Some(0)), 1);
        assert_eq!(clamp_agent_max_rows(Some(50)), 50);
        assert_eq!(clamp_agent_max_rows(Some(u32::MAX)), AGENT_MAX_ROWS);
    }
}

/// Safe connection card for models (no host / password / URI).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConnectionInfo {
    pub id: String,
    pub name: String,
    pub engine: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub read_only: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentObjectInfo {
    pub name: String,
    /// Schema-qualified name for SQL (`main.boards`). Same as `name` when schema is public/empty.
    pub qualified_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub row_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub pk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentQueryDownload {
    pub engine: String,
    pub columns: Vec<String>,
    pub row_count: u64,
    pub truncated: bool,
    pub duration_ms: u64,
    /// `json` rows or a CSV string.
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<Vec<JsonValue>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub csv: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: JsonValue,
}
