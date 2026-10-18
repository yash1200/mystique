use std::sync::Arc;

use mystique_core::{AppendGridEditHistoryInput, GridColumnLayoutRecord, GridEditHistoryRecord};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn get_grid_column_layout(
    state: State<'_, AppState>,
    connection_id: String,
    table_id: String,
) -> Result<Option<GridColumnLayoutRecord>, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .grid_layout()
            .get_layout(&connection_id, &table_id)
            .map_err(|err| err.to_string())
    })
    .await
}

#[tauri::command]
pub async fn save_grid_column_layout(
    state: State<'_, AppState>,
    layout: GridColumnLayoutRecord,
) -> Result<(), String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .grid_layout()
            .save_layout(layout)
            .map_err(|err| err.to_string())
    })
    .await
}

#[tauri::command]
pub async fn latest_grid_edit_history(
    state: State<'_, AppState>,
    connection_id: String,
    table_id: String,
) -> Result<Option<GridEditHistoryRecord>, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .grid_layout()
            .latest_history(&connection_id, &table_id)
            .map_err(|err| err.to_string())
    })
    .await
}

#[tauri::command]
pub async fn append_grid_edit_history(
    state: State<'_, AppState>,
    input: AppendGridEditHistoryInput,
) -> Result<i64, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .grid_layout()
            .append_history(input)
            .map_err(|err| err.to_string())
    })
    .await
}
