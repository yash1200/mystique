use std::sync::Arc;

use mystique_core::{AppendHistoryInput, HistoryList, QueryHistoryItem, QueryHistoryPage};
use tauri::State;

use super::IpcError;
use crate::AppState;

#[tauri::command]
pub async fn append_query_history(
    state: State<'_, AppState>,
    input: AppendHistoryInput,
) -> Result<QueryHistoryItem, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.history().append(input).map_err(IpcError::from)).await
}

#[tauri::command]
pub async fn list_query_history(
    state: State<'_, AppState>,
    connection_id: String,
    limit: Option<i64>,
    search: Option<String>,
    cursor_ran_at: Option<String>,
    cursor_id: Option<String>,
) -> Result<QueryHistoryPage, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        let limit = limit.unwrap_or(100);
        services
            .history()
            .list_page(HistoryList {
                connection_id: Some(connection_id),
                limit,
                search,
                cursor_ran_at,
                cursor_id,
            })
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn list_recent_query_history(
    state: State<'_, AppState>,
    limit: Option<i64>,
    search: Option<String>,
    cursor_ran_at: Option<String>,
    cursor_id: Option<String>,
) -> Result<QueryHistoryPage, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        let limit = limit.unwrap_or(100);
        services
            .history()
            .list_page(HistoryList {
                connection_id: None,
                limit,
                search,
                cursor_ran_at,
                cursor_id,
            })
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn clear_query_history(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<usize, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .history()
            .clear_for_connection(&connection_id)
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
/// Keep the latest `keep` rows on each connection. Inserts also enforce
/// [`mystique_core::QUERY_HISTORY_CAP`] (2_000) for the connection just written.
pub async fn retain_query_history(
    state: State<'_, AppState>,
    keep: i64,
) -> Result<usize, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .history()
            .retain_latest(keep)
            .map_err(IpcError::from)
    })
    .await
}
