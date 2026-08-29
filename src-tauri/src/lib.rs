//! Mystique Tauri shell.
//!
//! Responsibilities:
//! - Window / plugin lifecycle
//! - Open local DB path via Tauri path API
//! - IPC adapters → [`mystique_core::AppServices`]
//! - Full native app menu (Edit clipboard, View zoom, Window, workspace tab shortcuts)
//!
//! Business logic lives in `mystique-core`, not here.

mod ipc;

use std::sync::Arc;

use mystique_agent::AgentService;
use mystique_core::{AppDatabase, AppServices};
use mystique_mcp::McpService;
use tauri::{
    image::Image,
    menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu},
    Emitter, Manager,
};

const APP_ICON_PNG: &[u8] = include_bytes!("../icons/icon.png");

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

const APP_DISPLAY_NAME: &str = "Mystique";

fn about_metadata(app: &tauri::AppHandle) -> AboutMetadata<'_> {
    let pkg = app.package_info();
    let config = app.config();
    AboutMetadata {
        name: Some(APP_DISPLAY_NAME.into()),
        version: Some(pkg.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        comments: Some("Multi-engine database studio".into()),
        icon: Image::from_bytes(APP_ICON_PNG)
            .ok()
            .or_else(|| app.default_window_icon().cloned()),
        ..Default::default()
    }
}

fn build_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let about = about_metadata(app);

    // —— File (workspace + window) ————————————————————————————————
    // Do NOT use PredefinedMenuItem::close_window here: its accelerator is ⌘W
    // and would steal Close Tab. Close Window uses ⇧⌘W instead.
    let new_query = MenuItem::with_id(
        app,
        "workspace_new_query",
        "New Query",
        true,
        Some("CmdOrCtrl+T"),
    )?;
    let save_query = MenuItem::with_id(
        app,
        "workspace_save_query",
        "Save",
        true,
        Some("CmdOrCtrl+S"),
    )?;
    let close_tab = MenuItem::with_id(
        app,
        "workspace_close_tab",
        "Close Tab",
        true,
        Some("CmdOrCtrl+W"),
    )?;
    let close_window = MenuItem::with_id(
        app,
        "window_close",
        "Close Window",
        true,
        Some("Shift+CmdOrCtrl+W"),
    )?;

    #[cfg(target_os = "macos")]
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &new_query,
            &save_query,
            &close_tab,
            &PredefinedMenuItem::separator(app)?,
            &close_window,
        ],
    )?;

    #[cfg(not(target_os = "macos"))]
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &new_query,
            &save_query,
            &close_tab,
            &PredefinedMenuItem::separator(app)?,
            &close_window,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    // —— Edit (required for native clipboard / undo on macOS) ——————
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    // —— View (UI zoom + fullscreen) ———————————————————————————————
    // Use "=" not "Plus" — muda parses `CmdOrCtrl+=` / `CmdOrCtrl++` for zoom-in.
    let zoom_in = MenuItem::with_id(app, "view_zoom_in", "Zoom In", true, Some("CmdOrCtrl+="))?;
    let zoom_out = MenuItem::with_id(app, "view_zoom_out", "Zoom Out", true, Some("CmdOrCtrl+-"))?;
    let zoom_reset = MenuItem::with_id(
        app,
        "view_zoom_reset",
        "Actual Size",
        true,
        Some("CmdOrCtrl+0"),
    )?;

    #[cfg(target_os = "macos")]
    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &zoom_in,
            &zoom_out,
            &zoom_reset,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    #[cfg(not(target_os = "macos"))]
    let view_menu = Submenu::with_items(app, "View", true, &[&zoom_in, &zoom_out, &zoom_reset])?;

    // —— Window ————————————————————————————————————————————————————
    #[cfg(target_os = "macos")]
    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::bring_all_to_front(app, None)?,
        ],
    )?;

    #[cfg(not(target_os = "macos"))]
    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
        ],
    )?;

    // —— Help (About lives in the app menu on macOS) ———————————————
    #[cfg(not(target_os = "macos"))]
    let help_menu = Submenu::with_items(
        app,
        "Help",
        true,
        &[&PredefinedMenuItem::about(
            app,
            Some("About Mystique"),
            Some(about),
        )?],
    )?;

    // macOS: app-named first menu (About / Services / Hide / Quit).
    #[cfg(target_os = "macos")]
    {
        let app_menu = Submenu::with_items(
            app,
            APP_DISPLAY_NAME,
            true,
            &[
                &PredefinedMenuItem::about(app, Some("About Mystique"), Some(about))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::services(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::hide(app, Some("Hide Mystique"))?,
                &PredefinedMenuItem::hide_others(app, None)?,
                &PredefinedMenuItem::show_all(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::quit(app, Some("Quit Mystique"))?,
            ],
        )?;
        return Menu::with_items(
            app,
            &[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu],
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        Menu::with_items(
            app,
            &[&file_menu, &edit_menu, &view_menu, &window_menu, &help_menu],
        )
    }
}

fn emit_to_webviews(app: &tauri::AppHandle, event: &str) {
    for (_label, window) in app.webview_windows() {
        let _ = window.emit(event, ());
    }
    let _ = app.emit(event, ());
}

fn close_focused_window(app: &tauri::AppHandle) {
    if let Some(win) = app
        .webview_windows()
        .into_iter()
        .find_map(|(_label, w)| w.is_focused().ok().filter(|&f| f).map(|_| w))
    {
        let _ = win.close();
        return;
    }
    // Fallback: close the first window.
    if let Some((_label, win)) = app.webview_windows().into_iter().next() {
        let _ = win.close();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .menu(|app| build_menu(app))
        .on_menu_event(|app, event| match event.id().as_ref() {
            "workspace_close_tab" => emit_to_webviews(app, "workspace-close-tab"),
            "workspace_new_query" => emit_to_webviews(app, "workspace-new-query"),
            "workspace_save_query" => emit_to_webviews(app, "workspace-save-query"),
            "view_zoom_in" => emit_to_webviews(app, "ui-zoom-in"),
            "view_zoom_out" => emit_to_webviews(app, "ui-zoom-out"),
            "view_zoom_reset" => emit_to_webviews(app, "ui-zoom-reset"),
            "window_close" => close_focused_window(app),
            _ => {}
        })
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
            ipc::settings::get_setting,
            ipc::settings::set_setting,
            ipc::settings::list_settings,
            ipc::settings::delete_setting,
            // MCP (external clients). In-app agents must call AgentTools, not HTTP.
            ipc::mcp::get_mcp_status,
            ipc::mcp::set_mcp_enabled,
            ipc::mcp::rotate_mcp_token,
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
