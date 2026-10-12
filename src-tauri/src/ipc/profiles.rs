use mystique_core::{ConnectionProfile, SaveConnectionInput};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub fn list_profiles(state: State<'_, AppState>) -> Result<Vec<ConnectionProfile>, String> {
    super::guard_sync(|| state.services.profiles().list().map_err(|e| e.to_string()))
}

#[tauri::command]
pub fn get_profile(state: State<'_, AppState>, id: String) -> Result<ConnectionProfile, String> {
    super::guard_sync(|| {
        state
            .services
            .profiles()
            .get(&id)
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn save_profile(
    state: State<'_, AppState>,
    input: SaveConnectionInput,
) -> Result<ConnectionProfile, String> {
    super::guard_sync(|| {
        state
            .services
            .profiles()
            .save(input)
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn delete_profile(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    super::guard_sync(|| {
        if id.trim().is_empty() {
            return Err("connection id is required".into());
        }
        state
            .services
            .profiles()
            .delete(id.trim())
            .map_err(|e| e.to_string())?;
        Ok(true)
    })
}

#[tauri::command]
pub fn touch_profile_connected(
    state: State<'_, AppState>,
    id: String,
) -> Result<ConnectionProfile, String> {
    super::guard_sync(|| {
        state
            .services
            .profiles()
            .touch_connected(&id)
            .map_err(|e| e.to_string())
    })
}
