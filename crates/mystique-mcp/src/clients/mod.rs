//! Detect installed AI harnesses and write Mystique into their MCP config.

mod catalog;
mod formats;
mod host;

use serde::{Deserialize, Serialize};

pub use catalog::{install, list_available};
pub use host::Host;

pub const SERVER_NAME: &str = "mystique";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct McpEndpoint {
    pub url: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct McpHarness {
    pub id: String,
    pub label: String,
    pub mark: String,
    pub hint: String,
    pub config_path: String,
    pub installed: bool,
    pub stale: bool,
}
