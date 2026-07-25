use mystique_core::{AppendHistoryInput, QueryHistoryItem};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub fn append_query_history(
    state: State<'_, AppState>,
    input: AppendHistoryInput,
) -> Result<QueryHistoryItem, String> {
    state.services.history().append(input).map_err(Into::into)
}

#[tauri::command]
pub fn list_query_history(
    state: State<'_, AppState>,
    connection_id: String,
    limit: Option<i64>,
) -> Result<Vec<QueryHistoryItem>, String> {
    let limit = limit.unwrap_or(100);
    state
        .services
        .history()
        .list_for_connection(&connection_id, limit)
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_recent_query_history(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<QueryHistoryItem>, String> {
    let limit = limit.unwrap_or(100);
    state
        .services
        .history()
        .list_recent(limit)
        .map_err(Into::into)
}

#[tauri::command]
pub fn clear_query_history(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<usize, String> {
    state
        .services
        .history()
        .clear_for_connection(&connection_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn retain_query_history(state: State<'_, AppState>, keep: i64) -> Result<usize, String> {
    state
        .services
        .history()
        .retain_latest(keep)
        .map_err(Into::into)
}
