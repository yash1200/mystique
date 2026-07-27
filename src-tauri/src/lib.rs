//! MystiqueDB Tauri shell.
//!
//! Responsibilities:
//! - Window / plugin lifecycle
//! - Open local DB path via Tauri path API
//! - IPC adapters → [`mystique_core::AppServices`]
//! - App menu accelerators (⌘W close tab, ⌘T new query) so they don't close the window
//!
//! Business logic lives in `mystique-core`, not here.

mod ipc;

use std::sync::Arc;

use mystique_core::{AppDatabase, AppServices};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    Emitter, Manager,
};

/// Tauri-managed application state.
pub struct AppState {
    pub services: Arc<AppServices>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("app data dir: {e}"))?;
            std::fs::create_dir_all(&data_dir)?;

            let db_path = data_dir.join(mystique_core::storage::sqlite::db_file_name());
            let db = AppDatabase::open(&db_path).map_err(|e| e.to_string())?;
            let services = Arc::new(AppServices::new(db));

            app.manage(AppState { services });

            // Custom File menu: Cmd/Ctrl+W closes a workspace tab (not the window).
            // Cmd/Ctrl+T opens a new query tab. Events are handled in the webview.
            let new_query = MenuItem::with_id(
                app,
                "workspace_new_query",
                "New Query",
                true,
                Some("CmdOrCtrl+T"),
            )?;
            let close_tab = MenuItem::with_id(
                app,
                "workspace_close_tab",
                "Close Tab",
                true,
                Some("CmdOrCtrl+W"),
            )?;
            let file_menu = Submenu::with_items(
                app,
                "File",
                true,
                &[
                    &new_query,
                    &close_tab,
                    &PredefinedMenuItem::separator(app)?,
                    // Explicitly omit CloseWindow with Cmd+W — use Quit only.
                    &PredefinedMenuItem::quit(app, None)?,
                ],
            )?;
            let menu = Menu::with_items(app, &[&file_menu])?;
            app.set_menu(menu)?;

            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "workspace_close_tab" => {
                let _ = app.emit("workspace://close-tab", ());
            }
            "workspace_new_query" => {
                let _ = app.emit("workspace://new-query", ());
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            // engines / connection pools
            ipc::engines::list_engines,
            ipc::engines::test_engine_connection,
            ipc::engines::open_engine_session,
            ipc::engines::close_engine_session,
            ipc::engines::ping_engine_session,
            ipc::engines::engine_session_pool_status,
            ipc::engines::list_engine_sessions,
            ipc::engines::get_engine_session,
            ipc::engines::execute_engine_query,
            // profiles
            ipc::profiles::list_profiles,
            ipc::profiles::get_profile,
            ipc::profiles::save_profile,
            ipc::profiles::delete_profile,
            ipc::profiles::touch_profile_connected,
            // query library
            ipc::query_library::list_query_groups,
            ipc::query_library::save_query_group,
            ipc::query_library::delete_query_group,
            ipc::query_library::list_saved_queries,
            ipc::query_library::save_query,
            ipc::query_library::delete_saved_query,
            // history
            ipc::history::append_query_history,
            ipc::history::list_query_history,
            ipc::history::list_recent_query_history,
            ipc::history::clear_query_history,
            ipc::history::retain_query_history,
            // diagrams (ER)
            ipc::diagrams::list_diagrams,
            ipc::diagrams::get_diagram,
            ipc::diagrams::save_diagram,
            ipc::diagrams::delete_diagram,
            // settings
            ipc::settings::get_setting,
            ipc::settings::set_setting,
            ipc::settings::list_settings,
            ipc::settings::delete_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
