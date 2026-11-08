use std::sync::Arc;

use mystique_core::{GridColumnLayoutRecord, GridSaveSummary};
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

/// Latest save on this table that can still be rolled back. Id only.
#[tauri::command]
pub async fn latest_grid_save(
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
    schema: Option<String>,
    table: String,
) -> Result<Option<GridSaveSummary>, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .grid_layout()
            .latest_save(
                &connection_id,
                database.as_deref(),
                schema.as_deref(),
                &table,
            )
            .map_err(|err| err.to_string())
    })
    .await
}
