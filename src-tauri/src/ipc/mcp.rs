use mystique_mcp::McpStatus;
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn get_mcp_status(state: State<'_, AppState>) -> Result<McpStatus, String> {
    Ok(state.mcp.status().await)
}

#[tauri::command]
pub async fn set_mcp_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<McpStatus, String> {
    state
        .mcp
        .set_enabled(enabled)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rotate_mcp_token(state: State<'_, AppState>) -> Result<McpStatus, String> {
    state.mcp.rotate_token().await.map_err(|e| e.to_string())
}
