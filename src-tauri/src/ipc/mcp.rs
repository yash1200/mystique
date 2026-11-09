use std::sync::Arc;

use mystique_core::{ToolAccessView, ToolApprovalRequest};
use mystique_mcp::{McpHarness, McpStatus};
use tauri::State;

use super::IpcError;
use crate::AppState;

#[tauri::command]
pub async fn get_mcp_status(state: State<'_, AppState>) -> Result<McpStatus, IpcError> {
    super::guard(async move { Ok(state.mcp.status().await) }).await
}

#[tauri::command]
pub async fn set_mcp_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<McpStatus, IpcError> {
    super::guard(async move { state.mcp.set_enabled(enabled).await.map_err(IpcError::from) }).await
}

#[tauri::command]
pub async fn rotate_mcp_token(state: State<'_, AppState>) -> Result<McpStatus, IpcError> {
    super::guard(async move { state.mcp.rotate_token().await.map_err(IpcError::from) }).await
}

#[tauri::command]
pub async fn list_mcp_harnesses(state: State<'_, AppState>) -> Result<Vec<McpHarness>, IpcError> {
    super::guard(async move { Ok(state.mcp.list_harnesses().await) }).await
}

#[tauri::command]
pub async fn install_mcp_harness(
    state: State<'_, AppState>,
    id: String,
) -> Result<McpHarness, IpcError> {
    super::guard(async move { state.mcp.install_harness(&id).await.map_err(IpcError::from) }).await
}

/// Access levels for MCP and the built-in agent (Settings → MCP → Access).
#[tauri::command]
pub async fn get_tool_access(state: State<'_, AppState>) -> Result<ToolAccessView, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.tool_access().view().map_err(IpcError::from)).await
}

/// The global default for `caller` (`mcp` | `agent`).
#[tauri::command]
pub async fn set_tool_access_default(
    state: State<'_, AppState>,
    caller: String,
    level: String,
) -> Result<ToolAccessView, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .tool_access()
            .set_default(&caller, &level)
            .map_err(IpcError::from)
    })
    .await
}

/// A connection's level, or one database's. `level: null` inherits again.
#[tauri::command]
pub async fn set_tool_access_level(
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
    level: Option<String>,
) -> Result<ToolAccessView, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .tool_access()
            .set_level(&connection_id, database.as_deref(), level.as_deref())
            .map_err(IpcError::from)
    })
    .await
}

/// "Ask me for high-risk writes" on one connection.
#[tauri::command]
pub async fn set_tool_access_ask(
    state: State<'_, AppState>,
    connection_id: String,
    ask: bool,
) -> Result<ToolAccessView, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .tool_access()
            .set_ask(&connection_id, ask)
            .map_err(IpcError::from)
    })
    .await
}

/// Writes waiting in the approval dock, oldest first.
#[tauri::command]
pub fn list_tool_approvals(state: State<'_, AppState>) -> Vec<ToolApprovalRequest> {
    state.services.tool_approvals().pending()
}

/// Approve once, or deny. The request may already have timed out.
#[tauri::command]
pub fn decide_tool_approval(
    state: State<'_, AppState>,
    id: String,
    approve: bool,
) -> Result<(), IpcError> {
    super::guard_sync(|| {
        state
            .services
            .tool_approvals()
            .decide(&id, approve)
            .map_err(IpcError::from)
    })
}
