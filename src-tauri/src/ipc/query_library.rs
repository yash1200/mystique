use mystique_core::{SaveQueryInput, SavedQuery};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub fn list_saved_queries(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<SavedQuery>, String> {
    state
        .services
        .query_library()
        .list_queries(&connection_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_query(state: State<'_, AppState>, input: SaveQueryInput) -> Result<SavedQuery, String> {
    state
        .services
        .query_library()
        .save_query(input)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_saved_query(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .services
        .query_library()
        .delete_query(&id)
        .map_err(|e| e.to_string())
}
