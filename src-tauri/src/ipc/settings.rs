use mystique_core::AppSetting;
use tauri::State;

use crate::AppState;

#[tauri::command]
pub fn get_setting(state: State<'_, AppState>, key: String) -> Result<Option<AppSetting>, String> {
    state.services.settings().get(&key).map_err(Into::into)
}

#[tauri::command]
pub fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<AppSetting, String> {
    state
        .services
        .settings()
        .set(&key, &value)
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_settings(state: State<'_, AppState>) -> Result<Vec<AppSetting>, String> {
    state.services.settings().list().map_err(Into::into)
}

#[tauri::command]
pub fn delete_setting(state: State<'_, AppState>, key: String) -> Result<bool, String> {
    state.services.settings().delete(&key).map_err(Into::into)
}
