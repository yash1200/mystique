use mystique_core::{ErDiagram, SaveErDiagramInput};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub fn list_diagrams(
    state: State<'_, AppState>,
    family: Option<String>,
) -> Result<Vec<ErDiagram>, String> {
    state
        .services
        .diagrams()
        .list(family.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_diagram(state: State<'_, AppState>, id: String) -> Result<ErDiagram, String> {
    state
        .services
        .diagrams()
        .get(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_diagram(
    state: State<'_, AppState>,
    input: SaveErDiagramInput,
) -> Result<ErDiagram, String> {
    state
        .services
        .diagrams()
        .save(input)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_diagram(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .services
        .diagrams()
        .delete(&id)
        .map_err(|e| e.to_string())
}
