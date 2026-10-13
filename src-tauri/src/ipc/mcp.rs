use mystique_mcp::{McpAccess, McpHarness, McpStatus};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn get_mcp_status(state: State<'_, AppState>) -> Result<McpStatus, String> {
    super::guard(async move { Ok(state.mcp.status().await) }).await
}

#[tauri::command]
pub async fn set_mcp_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<McpStatus, String> {
    super::guard(async move {
        state
            .mcp
            .set_enabled(enabled)
            .await
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn rotate_mcp_token(state: State<'_, AppState>) -> Result<McpStatus, String> {
    super::guard(async move { state.mcp.rotate_token().await.map_err(|e| e.to_string()) }).await
}

#[tauri::command]
pub async fn list_mcp_harnesses(state: State<'_, AppState>) -> Result<Vec<McpHarness>, String> {
    super::guard(async move { Ok(state.mcp.list_harnesses().await) }).await
}

#[tauri::command]
pub async fn install_mcp_harness(
    state: State<'_, AppState>,
    id: String,
) -> Result<McpHarness, String> {
    super::guard(async move {
        state
            .mcp
            .install_harness(&id)
            .await
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_mcp_access(state: State<'_, AppState>) -> Result<McpAccess, String> {
    let mcp = state.mcp.clone();
    super::guard_blocking(move || mcp.access().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn set_mcp_access(
    state: State<'_, AppState>,
    policy: String,
    allowlist: Vec<String>,
) -> Result<McpAccess, String> {
    let mcp = state.mcp.clone();
    super::guard_blocking(move || {
        mcp.set_access(&policy, allowlist)
            .map_err(|e| e.to_string())
    })
    .await
}
