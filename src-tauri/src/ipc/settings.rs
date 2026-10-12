use mystique_core::AppSetting;
use tauri::State;

use crate::AppState;

#[tauri::command]
pub fn get_setting(state: State<'_, AppState>, key: String) -> Result<Option<AppSetting>, String> {
    super::guard_sync(|| {
        state
            .services
            .settings()
            .get(&key)
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<AppSetting, String> {
    super::guard_sync(|| {
        state
            .services
            .settings()
            .set(&key, &value)
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn list_settings(state: State<'_, AppState>) -> Result<Vec<AppSetting>, String> {
    super::guard_sync(|| state.services.settings().list().map_err(|e| e.to_string()))
}

#[tauri::command]
pub fn delete_setting(state: State<'_, AppState>, key: String) -> Result<bool, String> {
    super::guard_sync(|| {
        state
            .services
            .settings()
            .delete(&key)
            .map_err(|e| e.to_string())
    })
}
