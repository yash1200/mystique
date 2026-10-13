use std::sync::Arc;

use mystique_core::{AppendHistoryInput, QueryHistoryItem};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn append_query_history(
    state: State<'_, AppState>,
    input: AppendHistoryInput,
) -> Result<QueryHistoryItem, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.history().append(input).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn list_query_history(
    state: State<'_, AppState>,
    connection_id: String,
    limit: Option<i64>,
) -> Result<Vec<QueryHistoryItem>, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        let limit = limit.unwrap_or(100);
        services
            .history()
            .list_for_connection(&connection_id, limit)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn list_recent_query_history(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<QueryHistoryItem>, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        let limit = limit.unwrap_or(100);
        services
            .history()
            .list_recent(limit)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn clear_query_history(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<usize, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .history()
            .clear_for_connection(&connection_id)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn retain_query_history(state: State<'_, AppState>, keep: i64) -> Result<usize, String> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .history()
            .retain_latest(keep)
            .map_err(|e| e.to_string())
    })
    .await
}
