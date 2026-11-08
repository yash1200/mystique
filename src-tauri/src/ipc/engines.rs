//! Engine catalog + pooled session IPC (API-shaped).

use std::sync::Arc;

use mystique_core::{
    EngineDescriptor, FormatConnectionUrlRequest, FormattedConnectionUrl, OpenSessionRequest,
    ParseConnectionUrlRequest, ParsedConnectionUrl, PoolStatus, SessionInfo, SshHostKeyPrompt,
    TestResult,
};
use tauri::State;

use super::IpcError;
use crate::AppState;

#[tauri::command]
pub async fn list_engines(state: State<'_, AppState>) -> Result<Vec<EngineDescriptor>, IpcError> {
    super::guard(async move { Ok(state.services.connections().list_engines()) }).await
}

/// Parse a connection URL into discrete plugin config fields (plugin-owned).
#[tauri::command]
pub async fn parse_engine_connection_url(
    state: State<'_, AppState>,
    request: ParseConnectionUrlRequest,
) -> Result<ParsedConnectionUrl, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .parse_connection_url(&request.engine, &request.url)
            .map_err(IpcError::from)
    })
    .await
}

/// Format discrete plugin config into a connection URL (plugin-owned).
#[tauri::command]
pub async fn format_engine_connection_url(
    state: State<'_, AppState>,
    request: FormatConnectionUrlRequest,
) -> Result<FormattedConnectionUrl, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .format_connection_url(&request.engine, request.plugin_config, request.password)
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn trust_ssh_host_key(
    state: State<'_, AppState>,
    prompt: SshHostKeyPrompt,
) -> Result<(), IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.trust_ssh_host_key(prompt).map_err(IpcError::from)).await
}

#[tauri::command]
pub async fn test_engine_connection(
    state: State<'_, AppState>,
    request: OpenSessionRequest,
) -> Result<TestResult, IpcError> {
    super::guard(async move {
        let services = Arc::clone(&state.services);
        services
            .test_connection(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn open_engine_session(
    state: State<'_, AppState>,
    request: OpenSessionRequest,
) -> Result<SessionInfo, IpcError> {
    super::guard(async move {
        let services = Arc::clone(&state.services);
        services.open_session(request).await.map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn pin_query_tab(
    state: State<'_, AppState>,
    session_id: String,
    tab_id: String,
) -> Result<mystique_core::PinStatus, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .pin_tab(&session_id, &tab_id)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn unpin_query_tab(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<mystique_core::PinStatus, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .rollback_tab(&tab_id)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn commit_pinned_tab(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<mystique_core::PinStatus, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .commit_tab(&tab_id)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn rollback_pinned_tab(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<mystique_core::PinStatus, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .rollback_open_tab(&tab_id)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn pinned_tab_status(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<mystique_core::PinStatus, IpcError> {
    super::guard(async move { Ok(state.services.connections().pin_status(&tab_id).await) }).await
}

#[tauri::command]
pub async fn execute_pinned_batch(
    state: State<'_, AppState>,
    request: mystique_core::ExecuteQueryRequest,
    tab_id: String,
    on_progress: tauri::ipc::Channel<mystique_core::BatchEvent>,
) -> Result<mystique_core::QueryBatchResult, IpcError> {
    super::guard(async move {
        let progress = mystique_core::ProgressPump::new(move |event| {
            let _ = on_progress.send(event);
        });
        let connections = state.services.connections();
        match connections
            .execute_pinned_batch(request, tab_id.clone(), Some(progress))
            .await
        {
            Ok(mut result) => {
                mystique_core::clip_batch_result(&mut result);
                Ok(result)
            }
            // The tab learns whether its transaction survived the failure.
            Err(err) => {
                Err(IpcError::from(err)
                    .with_transaction_outcome(connections.pin_status(&tab_id).await))
            }
        }
    })
    .await
}

#[tauri::command]
pub async fn close_engine_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .close_session(&session_id)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn ping_engine_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .ping(&session_id)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn engine_session_pool_status(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<PoolStatus>, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .pool_status(&session_id)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn list_engine_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<SessionInfo>, IpcError> {
    super::guard(async move { Ok(state.services.connections().list_sessions().await) }).await
}

#[tauri::command]
pub async fn get_engine_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<SessionInfo>, IpcError> {
    super::guard(async move { Ok(state.services.connections().get_session(&session_id).await) })
        .await
}

#[tauri::command]
pub async fn unlock_engine_writes(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .unlock_writes(&session_id)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn cancel_engine_query(
    state: State<'_, AppState>,
    execution_id: String,
) -> Result<(), IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .cancel_query(&execution_id)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn explain_engine_query(
    state: State<'_, AppState>,
    request: mystique_core::ExplainQueryRequest,
) -> Result<mystique_core::ExplainPlan, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .explain_query(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn execute_engine_query(
    state: State<'_, AppState>,
    request: mystique_core::ExecuteQueryRequest,
) -> Result<mystique_core::QueryResult, IpcError> {
    super::guard(async move {
        let mut result = state.services.connections().execute_query(request).await?;
        mystique_core::clip_query_result(&mut result);
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn execute_engine_batch(
    state: State<'_, AppState>,
    request: mystique_core::ExecuteQueryRequest,
    on_progress: tauri::ipc::Channel<mystique_core::BatchEvent>,
) -> Result<mystique_core::QueryBatchResult, IpcError> {
    super::guard(async move {
        let progress = mystique_core::ProgressPump::new(move |event| {
            let _ = on_progress.send(event);
        });
        let mut result = state
            .services
            .connections()
            .execute_batch(request, Some(progress))
            .await?;
        mystique_core::clip_batch_result(&mut result);
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn describe_engine_table(
    state: State<'_, AppState>,
    request: mystique_core::DescribeTableRequest,
) -> Result<Vec<mystique_core::ColumnInfo>, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .describe_table(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn list_engine_catalog_columns(
    state: State<'_, AppState>,
    request: mystique_core::ListCatalogColumnsRequest,
) -> Result<Vec<mystique_core::CatalogColumnRow>, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .list_catalog_columns(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn apply_engine_table_edits(
    state: State<'_, AppState>,
    request: mystique_core::ApplyTableEditsRequest,
) -> Result<mystique_core::ApplyTableEditsResult, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .apply_table_edits(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn distinct_engine_column_values(
    state: State<'_, AppState>,
    request: mystique_core::DistinctColumnValuesRequest,
) -> Result<Vec<serde_json::Value>, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .distinct_column_values(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn rollback_engine_table_edits(
    state: State<'_, AppState>,
    request: mystique_core::RollbackTableEditsRequest,
) -> Result<(), IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .rollback_table_edits(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn fetch_engine_table_preview(
    state: State<'_, AppState>,
    request: mystique_core::FetchTablePreviewRequest,
) -> Result<mystique_core::TablePreviewResult, IpcError> {
    super::guard(async move {
        let database = request.database.clone();
        let schema = request.schema.clone();
        let table = request.table.clone();
        let mut result = state
            .services
            .connections()
            .fetch_table_preview(request)
            .await
            .map_err(IpcError::from)?;
        mystique_core::clip_table_preview(
            &mut result,
            database.as_deref(),
            schema.as_deref(),
            &table,
        );
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn fetch_engine_large_value(
    state: State<'_, AppState>,
    request: mystique_core::FetchLargeValueRequest,
) -> Result<mystique_core::FetchLargeValueResult, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .fetch_large_value(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn fetch_engine_table_preview_stats(
    state: State<'_, AppState>,
    request: mystique_core::FetchTablePreviewStatsRequest,
) -> Result<mystique_core::TablePreviewStats, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .fetch_table_preview_stats(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn list_engine_catalog(
    state: State<'_, AppState>,
    request: mystique_core::ListCatalogRequest,
) -> Result<Vec<mystique_core::CatalogDatabase>, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .list_catalog(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn list_engine_relationships(
    state: State<'_, AppState>,
    request: mystique_core::ListRelationshipsRequest,
) -> Result<Vec<mystique_core::CatalogRelationship>, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .list_relationships(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn preview_engine_import(
    state: State<'_, AppState>,
    request: mystique_core::PreviewImportRequest,
) -> Result<mystique_core::PreviewImportResult, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .preview_import(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn import_engine_object(
    state: State<'_, AppState>,
    request: mystique_core::ImportObjectRequest,
) -> Result<mystique_core::ImportObjectResult, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .import_object(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn export_engine_object(
    state: State<'_, AppState>,
    request: mystique_core::ExportObjectRequest,
) -> Result<mystique_core::ExportObjectResult, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .export_object(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn apply_engine_table_admin(
    state: State<'_, AppState>,
    request: mystique_core::TableAdminRequest,
) -> Result<mystique_core::TableAdminResult, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .apply_table_admin(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn describe_engine_table_definition(
    state: State<'_, AppState>,
    request: mystique_core::DescribeTableDefinitionRequest,
) -> Result<mystique_core::TableDefinition, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .describe_table_definition(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn list_engine_table_definitions(
    state: State<'_, AppState>,
    request: mystique_core::ListTableDefinitionsRequest,
) -> Result<Vec<mystique_core::TableDefinition>, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .list_table_definitions(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn plan_engine_ddl(
    state: State<'_, AppState>,
    request: mystique_core::PlanDdlRequest,
) -> Result<mystique_core::DdlPlan, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .plan_ddl(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn apply_engine_ddl(
    state: State<'_, AppState>,
    request: mystique_core::ApplyDdlRequest,
) -> Result<mystique_core::ApplyDdlResult, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .apply_ddl(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn plan_engine_schema_diff(
    state: State<'_, AppState>,
    request: mystique_core::PlanSchemaDiffRequest,
) -> Result<mystique_core::SchemaMigration, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .plan_schema_diff(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn apply_engine_schema_diff(
    state: State<'_, AppState>,
    request: mystique_core::ApplySchemaDiffRequest,
) -> Result<mystique_core::ApplyDdlResult, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .apply_schema_diff(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn describe_engine_object_structure(
    state: State<'_, AppState>,
    request: mystique_core::ObjectStructureRequest,
) -> Result<mystique_core::ObjectStructure, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .describe_object_structure(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn fetch_engine_server_metrics(
    state: State<'_, AppState>,
    request: mystique_core::FetchServerMetricsRequest,
) -> Result<mystique_core::ServerMetricsSnapshot, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .fetch_server_metrics(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn terminate_engine_session(
    state: State<'_, AppState>,
    request: mystique_core::TerminateSessionRequest,
) -> Result<mystique_core::TerminateSessionResult, IpcError> {
    super::guard(async move {
        state
            .services
            .connections()
            .terminate_backend_session(request)
            .await
            .map_err(IpcError::from)
    })
    .await
}
