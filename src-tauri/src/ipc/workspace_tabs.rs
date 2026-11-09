use std::sync::Arc;

use mystique_core::{ReplaceWorkspaceTabsInput, SaveWorkspaceTabInput, WorkspaceTabRecord};
use tauri::State;

use super::IpcError;
use crate::AppState;

#[tauri::command]
pub async fn list_workspace_tabs(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<WorkspaceTabRecord>, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .workspace_tabs()
            .list_tabs(&connection_id)
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn replace_workspace_tabs(
    state: State<'_, AppState>,
    input: ReplaceWorkspaceTabsInput,
) -> Result<Vec<WorkspaceTabRecord>, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .workspace_tabs()
            .replace_tabs(input)
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn upsert_workspace_tab(
    state: State<'_, AppState>,
    input: SaveWorkspaceTabInput,
) -> Result<WorkspaceTabRecord, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .workspace_tabs()
            .upsert_tab(input)
            .map_err(IpcError::from)
    })
    .await
}
