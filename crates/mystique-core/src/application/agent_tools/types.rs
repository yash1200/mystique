use mystique_connector_api::HARD_MAX_ROWS;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

pub const AGENT_MAX_ROWS: u32 = 2_000;
pub const AGENT_DEFAULT_ROWS: u32 = 200;

/// Characters of each cell returned to the in-app agent and MCP.
pub const AGENT_DEFAULT_CELL_CHARS: u32 = 200;
/// A request may ask for a wider window, but not past this.
pub const AGENT_MAX_CELL_CHARS: u32 = 4_000;
/// Caps a sliding read so a huge offset cannot walk off into nowhere.
pub const AGENT_MAX_CELL_OFFSET: u32 = 1_000_000;

/// Character window applied to every cell in a model-bound tool result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentCellWindow {
    pub offset: u32,
    pub limit: u32,
}

impl Default for AgentCellWindow {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: AGENT_DEFAULT_CELL_CHARS,
        }
    }
}

impl AgentCellWindow {
    pub fn from_options(offset: Option<u64>, limit: Option<u64>) -> Self {
        Self {
            offset: bounded_cell_option(offset, 0, AGENT_MAX_CELL_OFFSET),
            limit: bounded_cell_option(limit, AGENT_DEFAULT_CELL_CHARS, AGENT_MAX_CELL_CHARS)
                .max(1),
        }
    }

    pub fn from_args(args: &JsonValue) -> Self {
        Self::from_options(
            args.get("cell_char_offset").and_then(|v| v.as_u64()),
            args.get("cell_char_limit").and_then(|v| v.as_u64()),
        )
    }
}

fn bounded_cell_option(value: Option<u64>, default: u32, maximum: u32) -> u32 {
    match value {
        Some(n) => u32::try_from(n).unwrap_or(u32::MAX).min(maximum),
        None => default,
    }
}

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

    #[test]
    fn cell_window_defaults_and_clamps() {
        assert_eq!(
            AgentCellWindow::from_options(None, None),
            AgentCellWindow::default()
        );
        assert_eq!(AgentCellWindow::default().limit, AGENT_DEFAULT_CELL_CHARS);
        assert_eq!(
            AgentCellWindow::from_options(None, Some(100_000)).limit,
            AGENT_MAX_CELL_CHARS
        );
        assert_eq!(AgentCellWindow::from_options(None, Some(0)).limit, 1);
        assert_eq!(
            AgentCellWindow::from_options(Some(u64::MAX), Some(u64::MAX)),
            AgentCellWindow {
                offset: AGENT_MAX_CELL_OFFSET,
                limit: AGENT_MAX_CELL_CHARS,
            }
        );
    }
}
