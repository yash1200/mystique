//! Engine catalog + pooled session IPC (API-shaped).

use mystique_core::{EngineDescriptor, OpenSessionRequest, PoolStatus, SessionInfo, TestResult};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn list_engines(state: State<'_, AppState>) -> Result<Vec<EngineDescriptor>, String> {
    Ok(state.services.connections().list_engines())
}

#[tauri::command]
pub async fn test_engine_connection(
    state: State<'_, AppState>,
    request: OpenSessionRequest,
) -> Result<TestResult, String> {
    state
        .services
        .connections()
        .test_connection(&request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_engine_session(
    state: State<'_, AppState>,
    request: OpenSessionRequest,
) -> Result<SessionInfo, String> {
    state
        .services
        .connections()
        .open_session(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn close_engine_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state
        .services
        .connections()
        .close_session(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ping_engine_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state
        .services
        .connections()
        .ping(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn engine_session_pool_status(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<PoolStatus>, String> {
    state
        .services
        .connections()
        .pool_status(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_engine_sessions(state: State<'_, AppState>) -> Result<Vec<SessionInfo>, String> {
    Ok(state.services.connections().list_sessions().await)
}

#[tauri::command]
pub async fn get_engine_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<SessionInfo>, String> {
    Ok(state.services.connections().get_session(&session_id).await)
}

#[tauri::command]
pub async fn execute_engine_query(
    state: State<'_, AppState>,
    request: mystique_core::ExecuteQueryRequest,
) -> Result<mystique_core::QueryResult, String> {
    state
        .services
        .connections()
        .execute_query(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn describe_engine_table(
    state: State<'_, AppState>,
    request: mystique_core::DescribeTableRequest,
) -> Result<Vec<mystique_core::ColumnInfo>, String> {
    state
        .services
        .connections()
        .describe_table(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_engine_catalog_columns(
    state: State<'_, AppState>,
    request: mystique_core::ListCatalogColumnsRequest,
) -> Result<Vec<mystique_core::CatalogColumnRow>, String> {
    state
        .services
        .connections()
        .list_catalog_columns(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_engine_table_edits(
    state: State<'_, AppState>,
    request: mystique_core::ApplyTableEditsRequest,
) -> Result<mystique_core::ApplyTableEditsResult, String> {
    state
        .services
        .connections()
        .apply_table_edits(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn fetch_engine_table_preview(
    state: State<'_, AppState>,
    request: mystique_core::FetchTablePreviewRequest,
) -> Result<mystique_core::TablePreviewResult, String> {
    state
        .services
        .connections()
        .fetch_table_preview(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_engine_catalog(
    state: State<'_, AppState>,
    request: mystique_core::ListCatalogRequest,
) -> Result<Vec<mystique_core::CatalogDatabase>, String> {
    state
        .services
        .connections()
        .list_catalog(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_engine_relationships(
    state: State<'_, AppState>,
    request: mystique_core::ListRelationshipsRequest,
) -> Result<Vec<mystique_core::CatalogRelationship>, String> {
    state
        .services
        .connections()
        .list_relationships(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_engine_table_admin(
    state: State<'_, AppState>,
    request: mystique_core::TableAdminRequest,
) -> Result<mystique_core::TableAdminResult, String> {
    state
        .services
        .connections()
        .apply_table_admin(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn describe_engine_object_structure(
    state: State<'_, AppState>,
    request: mystique_core::ObjectStructureRequest,
) -> Result<mystique_core::ObjectStructure, String> {
    state
        .services
        .connections()
        .describe_object_structure(request)
        .await
        .map_err(|e| e.to_string())
}
