use std::sync::Arc;

use mystique_core::{AppSetting, PurgeReport, VaultStatus};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn get_setting(
    state: State<'_, AppState>,
    key: String,
) -> Result<Option<AppSetting>, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.settings().get(&key).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<AppSetting, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .settings()
            .set(&key, &value)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn list_settings(state: State<'_, AppState>) -> Result<Vec<AppSetting>, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.settings().list().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn delete_setting(state: State<'_, AppState>, key: String) -> Result<bool, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.settings().delete(&key).map_err(|e| e.to_string())).await
}

/// Never loads the vault key.
#[tauri::command]
pub async fn get_vault_status(state: State<'_, AppState>) -> Result<VaultStatus, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || Ok(services.vault().status())).await
}

/// Settings > Retry keychain: forget a cached denial and ask again.
#[tauri::command]
pub async fn retry_vault_unlock(state: State<'_, AppState>) -> Result<VaultStatus, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || Ok(services.vault().retry())).await
}

/// Settings > Clean up old keychain items. Never shows keychain UI.
#[tauri::command]
pub async fn purge_keychain_items(state: State<'_, AppState>) -> Result<PurgeReport, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.vault().purge_keychain().map_err(|e| e.to_string()))
        .await
}
