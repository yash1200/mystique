use mystique_core::{ReplaceWorkspaceTabsInput, WorkspaceTabRecord};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub fn list_workspace_tabs(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<WorkspaceTabRecord>, String> {
    super::guard_sync(|| {
        state
            .services
            .workspace_tabs()
            .list_tabs(&connection_id)
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn replace_workspace_tabs(
    state: State<'_, AppState>,
    input: ReplaceWorkspaceTabsInput,
) -> Result<Vec<WorkspaceTabRecord>, String> {
    super::guard_sync(|| {
        state
            .services
            .workspace_tabs()
            .replace_tabs(input)
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn delete_workspace_tab(state: State<'_, AppState>, id: String) -> Result<(), String> {
    super::guard_sync(|| {
        state
            .services
            .workspace_tabs()
            .delete_tab(&id)
            .map_err(|e| e.to_string())
    })
}
