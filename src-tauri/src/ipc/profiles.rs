use std::sync::Arc;

use mystique_core::{ConnectionProfile, SaveConnectionInput};
use tauri::State;

use super::IpcError;
use crate::AppState;

#[tauri::command]
pub async fn list_profiles(state: State<'_, AppState>) -> Result<Vec<ConnectionProfile>, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.profiles().list().map_err(IpcError::from)).await
}

#[tauri::command]
pub async fn save_profile(
    state: State<'_, AppState>,
    input: SaveConnectionInput,
) -> Result<ConnectionProfile, IpcError> {
    let services = Arc::clone(&state.services);
    let saved =
        super::guard_blocking(move || services.profiles().save(input).map_err(IpcError::from))
            .await?;
    // Open sessions follow the stored read-only bit; requests carry none.
    state
        .services
        .connections()
        .apply_profile_read_only(&saved.id, saved.read_only)
        .await;
    Ok(saved)
}

/// Mark one database of a profile with an environment, or clear its mark.
#[tauri::command]
pub async fn set_profile_database_environment(
    state: State<'_, AppState>,
    input: mystique_core::SetDatabaseEnvironmentInput,
) -> Result<ConnectionProfile, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .set_database_environment(&input)
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, id: String) -> Result<bool, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        if id.trim().is_empty() {
            return Err(IpcError::invalid("connection id is required"));
        }
        services
            .profiles()
            .delete(id.trim())
            .map_err(IpcError::from)?;
        Ok(true)
    })
    .await
}

#[tauri::command]
pub async fn touch_profile_connected(
    state: State<'_, AppState>,
    id: String,
) -> Result<ConnectionProfile, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .profiles()
            .touch_connected(&id)
            .map_err(IpcError::from)
    })
    .await
}
