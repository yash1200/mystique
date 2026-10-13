use std::sync::Arc;

use mystique_core::{ConnectionProfile, SaveConnectionInput};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn list_profiles(state: State<'_, AppState>) -> Result<Vec<ConnectionProfile>, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.profiles().list().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn get_profile(
    state: State<'_, AppState>,
    id: String,
) -> Result<ConnectionProfile, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.profiles().get(&id).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn save_profile(
    state: State<'_, AppState>,
    input: SaveConnectionInput,
) -> Result<ConnectionProfile, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.profiles().save(input).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        if id.trim().is_empty() {
            return Err("connection id is required".into());
        }
        services
            .profiles()
            .delete(id.trim())
            .map_err(|e| e.to_string())?;
        Ok(true)
    })
    .await
}

#[tauri::command]
pub async fn touch_profile_connected(
    state: State<'_, AppState>,
    id: String,
) -> Result<ConnectionProfile, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .profiles()
            .touch_connected(&id)
            .map_err(|e| e.to_string())
    })
    .await
}
