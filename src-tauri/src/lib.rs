//! Mystique Tauri shell.
//!
//! Responsibilities:
//! - Window / plugin lifecycle
//! - Open local DB path via Tauri path API
//! - IPC adapters → [`mystique_core::AppServices`]
//! - Page-aware native app menu (Home vs Workspace)
//!
//! Business logic lives in `mystique-core`, not here.

mod ipc;
mod menu;

use std::sync::Arc;

use mystique_agent::AgentService;
use mystique_core::{AppDatabase, AppServices};
use mystique_mcp::McpService;
use tauri::{image::Image, Manager};

use crate::menu::NativeMenuState;

pub(crate) const APP_ICON_PNG: &[u8] = include_bytes!("../icons/icon.png");
pub(crate) const APP_DISPLAY_NAME: &str = "Mystique";

fn apply_app_icon(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let icon = Image::from_bytes(APP_ICON_PNG)?;
    if let Some(win) = app.get_webview_window("main") {
        win.set_icon(icon.clone())?;
    }
    for (_label, win) in app.webview_windows() {
        let _ = win.set_icon(icon.clone());
    }
    Ok(())
}

/// Tauri-managed application state.
pub struct AppState {
    pub services: Arc<AppServices>,
    pub mcp: McpService,
    pub agent: AgentService,
}

#[tauri::command]
fn set_native_menu(app: tauri::AppHandle, state: NativeMenuState) -> Result<(), String> {
    menu::apply_menu(&app, &state).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .menu(|app| menu::build_menu(app, &NativeMenuState::default()))
        .on_menu_event(|app, event| menu::handle_menu_event(app, event.id().as_ref()))
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("app data dir: {e}"))?;
            std::fs::create_dir_all(&data_dir)?;

            let db_path = data_dir.join(mystique_core::storage::sqlite::db_file_name());
            let db = AppDatabase::open(&db_path).map_err(|e| e.to_string())?;
            let services = Arc::new(AppServices::new(db));
            let mcp = McpService::new(Arc::clone(&services));
            let agent = AgentService::with_legacy_file(
                Arc::clone(&services),
                data_dir.join("ai_secrets.json"),
            );
            let boot = mcp.clone();
            tauri::async_runtime::spawn(async move {
                boot.restore_if_enabled().await;
            });

            app.manage(AppState {
                services,
                mcp,
                agent,
            });
            apply_app_icon(app).map_err(|e| format!("app icon: {e}"))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // engines / connection pools
            ipc::engines::list_engines,
            ipc::engines::parse_engine_connection_url,
            ipc::engines::format_engine_connection_url,
            ipc::engines::test_engine_connection,
            ipc::engines::trust_ssh_host_key,
            ipc::engines::open_engine_session,
            ipc::engines::close_engine_session,
            ipc::engines::ping_engine_session,
            ipc::engines::engine_session_pool_status,
            ipc::engines::list_engine_sessions,
            ipc::engines::get_engine_session,
            ipc::engines::execute_engine_query,
            ipc::engines::describe_engine_table,
            ipc::engines::list_engine_catalog_columns,
            ipc::engines::apply_engine_table_edits,
            ipc::engines::fetch_engine_table_preview,
            ipc::engines::fetch_engine_table_preview_stats,
            ipc::engines::list_engine_catalog,
            ipc::engines::list_engine_relationships,
            ipc::engines::apply_engine_table_admin,
            ipc::engines::export_engine_object,
            ipc::engines::preview_engine_import,
            ipc::engines::import_engine_object,
            ipc::engines::describe_engine_object_structure,
            ipc::engines::fetch_engine_server_metrics,
            ipc::engines::terminate_engine_session,
            // profiles
            ipc::profiles::list_profiles,
            ipc::profiles::get_profile,
            ipc::profiles::save_profile,
            ipc::profiles::delete_profile,
            ipc::profiles::touch_profile_connected,
            // query library (per-connection, no groups)
            ipc::query_library::list_saved_queries,
            ipc::query_library::save_query,
            ipc::query_library::delete_saved_query,
            // workspace tabs (restored on reconnect)
            ipc::workspace_tabs::list_workspace_tabs,
            ipc::workspace_tabs::replace_workspace_tabs,
            ipc::workspace_tabs::delete_workspace_tab,
            // history
            ipc::history::append_query_history,
            ipc::history::list_query_history,
            ipc::history::list_recent_query_history,
            ipc::history::clear_query_history,
            ipc::history::retain_query_history,
            // settings
            set_native_menu,
            ipc::settings::get_setting,
            ipc::settings::set_setting,
            ipc::settings::list_settings,
            ipc::settings::delete_setting,
            // MCP (external clients). In-app agents must call AgentTools, not HTTP.
            ipc::mcp::get_mcp_status,
            ipc::mcp::set_mcp_enabled,
            ipc::mcp::rotate_mcp_token,
            ipc::mcp::list_mcp_harnesses,
            ipc::mcp::install_mcp_harness,
            // in-app query-tab agent
            ipc::agent::get_ai_settings,
            ipc::agent::set_ai_enabled,
            ipc::agent::set_ai_ollama_url,
            ipc::agent::set_ai_api_key,
            ipc::agent::list_agent_providers,
            ipc::agent::list_agent_models,
            ipc::agent::list_agent_chats,
            ipc::agent::create_agent_chat,
            ipc::agent::delete_agent_chat,
            ipc::agent::list_agent_messages,
            ipc::agent::update_agent_chat,
            ipc::agent::send_agent_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
