use mystique_core::{QueryGroup, SaveQueryGroupInput, SaveQueryInput, SavedQuery};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub fn list_query_groups(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<QueryGroup>, String> {
    state
        .services
        .query_library()
        .list_groups(&connection_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn save_query_group(
    state: State<'_, AppState>,
    input: SaveQueryGroupInput,
) -> Result<QueryGroup, String> {
    state
        .services
        .query_library()
        .save_group(input)
        .map_err(Into::into)
}

#[tauri::command]
pub fn delete_query_group(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .services
        .query_library()
        .delete_group(&id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_saved_queries(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<SavedQuery>, String> {
    state
        .services
        .query_library()
        .list_queries(&connection_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn save_query(state: State<'_, AppState>, input: SaveQueryInput) -> Result<SavedQuery, String> {
    state
        .services
        .query_library()
        .save_query(input)
        .map_err(Into::into)
}

#[tauri::command]
pub fn delete_saved_query(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .services
        .query_library()
        .delete_query(&id)
        .map_err(Into::into)
}
