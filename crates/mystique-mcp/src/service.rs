//! MCP lifecycle: persist enable/token/port, start/stop the HTTP server.

use std::sync::Arc;

use mystique_core::{AppResult, AppServices};

use crate::server::{ensure_token, McpController, McpStatus, DEFAULT_MCP_PORT};

pub const SETTING_MCP_ENABLED: &str = "mcp.enabled";
pub const SETTING_MCP_PORT: &str = "mcp.port";
pub const SETTING_MCP_TOKEN: &str = "mcp.token";

/// Host-facing MCP service. Uses [`AppServices`] for tools + settings only.
#[derive(Clone)]
pub struct McpService {
    app: Arc<AppServices>,
    controller: McpController,
}

impl McpService {
    pub fn new(app: Arc<AppServices>) -> Self {
        Self {
            app,
            controller: McpController::new(),
        }
    }

    pub async fn status(&self) -> McpStatus {
        self.controller.status(self.enabled_setting()).await
    }

    pub fn enabled_setting(&self) -> bool {
        self.app
            .settings()
            .get(SETTING_MCP_ENABLED)
            .ok()
            .flatten()
            .map(|s| s.value == "true" || s.value == "1")
            .unwrap_or(false)
    }

    pub fn port_setting(&self) -> u16 {
        self.app
            .settings()
            .get(SETTING_MCP_PORT)
            .ok()
            .flatten()
            .and_then(|s| s.value.parse().ok())
            .filter(|p| *p > 0)
            .unwrap_or(DEFAULT_MCP_PORT)
    }

    fn token_setting(&self) -> String {
        let existing = self
            .app
            .settings()
            .get(SETTING_MCP_TOKEN)
            .ok()
            .flatten()
            .map(|s| s.value);
        let token = ensure_token(existing.as_deref());
        if existing.as_deref() != Some(token.as_str()) {
            let _ = self.app.settings().set(SETTING_MCP_TOKEN, &token);
        }
        token
    }

    pub async fn start(&self) -> AppResult<McpStatus> {
        let _ = self.app.settings().set(SETTING_MCP_ENABLED, "true");
        self.controller
            .start(
                self.app.agent_tools(),
                self.port_setting(),
                self.token_setting(),
            )
            .await
    }

    pub async fn stop(&self) -> AppResult<McpStatus> {
        let _ = self.app.settings().set(SETTING_MCP_ENABLED, "false");
        self.controller.stop().await;
        Ok(self.controller.status(false).await)
    }

    pub async fn set_enabled(&self, enabled: bool) -> AppResult<McpStatus> {
        if enabled {
            self.start().await
        } else {
            self.stop().await
        }
    }

    pub async fn rotate_token(&self) -> AppResult<McpStatus> {
        let token = ensure_token(None);
        let _ = self.app.settings().set(SETTING_MCP_TOKEN, &token);
        if self.enabled_setting() {
            self.start().await
        } else {
            Ok(self.controller.status(false).await)
        }
    }

    pub async fn restore_if_enabled(&self) {
        if self.enabled_setting() {
            let _ = self.start().await;
        }
    }
}
