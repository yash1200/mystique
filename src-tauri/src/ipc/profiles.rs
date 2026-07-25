use mystique_core::{ConnectionProfile, SaveConnectionInput};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub fn list_profiles(state: State<'_, AppState>) -> Result<Vec<ConnectionProfile>, String> {
    state.services.profiles().list().map_err(Into::into)
}

#[tauri::command]
pub fn get_profile(state: State<'_, AppState>, id: String) -> Result<ConnectionProfile, String> {
    state.services.profiles().get(&id).map_err(Into::into)
}

#[tauri::command]
pub fn save_profile(
    state: State<'_, AppState>,
    input: SaveConnectionInput,
) -> Result<ConnectionProfile, String> {
    state.services.profiles().save(input).map_err(Into::into)
}

#[tauri::command]
pub fn delete_profile(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.services.profiles().delete(&id).map_err(Into::into)
}

#[tauri::command]
pub fn touch_profile_connected(
    state: State<'_, AppState>,
    id: String,
) -> Result<ConnectionProfile, String> {
    state
        .services
        .profiles()
        .touch_connected(&id)
        .map_err(Into::into)
}
