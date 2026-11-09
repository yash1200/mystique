use std::sync::Arc;

use mystique_core::{PurgeReport, VaultStatus};
use tauri::State;

use super::IpcError;
use crate::AppState;

/// Never loads the vault key.
#[tauri::command]
pub async fn get_vault_status(state: State<'_, AppState>) -> Result<VaultStatus, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || Ok(services.vault().status())).await
}

/// Settings > Retry keychain: forget a cached denial and ask again.
#[tauri::command]
pub async fn retry_vault_unlock(state: State<'_, AppState>) -> Result<VaultStatus, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || Ok(services.vault().retry())).await
}

/// Settings > Clean up old keychain items. Never shows keychain UI.
#[tauri::command]
pub async fn purge_keychain_items(state: State<'_, AppState>) -> Result<PurgeReport, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.vault().purge_keychain().map_err(IpcError::from)).await
}
