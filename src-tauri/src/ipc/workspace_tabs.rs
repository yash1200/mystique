use std::sync::Arc;

use mystique_core::{ReplaceWorkspaceTabsInput, WorkspaceTabRecord};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn list_workspace_tabs(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<WorkspaceTabRecord>, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .workspace_tabs()
            .list_tabs(&connection_id)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn replace_workspace_tabs(
    state: State<'_, AppState>,
    input: ReplaceWorkspaceTabsInput,
) -> Result<Vec<WorkspaceTabRecord>, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .workspace_tabs()
            .replace_tabs(input)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_workspace_tab(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .workspace_tabs()
            .delete_tab(&id)
            .map_err(|e| e.to_string())
    })
    .await
}
