use std::sync::Arc;

use mystique_core::{SaveQueryInput, SavedQuery};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn list_saved_queries(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<SavedQuery>, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .query_library()
            .list_queries(&connection_id)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn save_query(
    state: State<'_, AppState>,
    input: SaveQueryInput,
) -> Result<SavedQuery, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .query_library()
            .save_query(input)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_saved_query(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .query_library()
            .delete_query(&id)
            .map_err(|e| e.to_string())
    })
    .await
}
