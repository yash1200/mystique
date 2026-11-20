//! Mystique Tauri shell.
//!
//! Responsibilities:
//! - Window / plugin lifecycle
//! - Open local DB path via Tauri path API
//! - IPC adapters → [`mystique_core::AppServices`]
//! - Page-aware native app menu on macOS (Home vs Workspace); Windows and
//!   Linux draw theirs in the custom titlebar
//! - Logging (`MYSTIQUE_LOG`, app log dir) — see [`logging`]
//!
//! Business logic lives in `mystique-core`, not here.

mod ipc;
mod logging;
#[cfg(target_os = "macos")]
mod menu;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mystique_acp::AcpService;
use mystique_agent::AgentService;
use mystique_core::{AppDatabase, AppServices};
use mystique_mcp::McpService;
use tauri::{image::Image, Emitter, Manager};

pub(crate) const APP_ICON_PNG: &[u8] = include_bytes!("../icons/icon.png");
#[cfg(target_os = "macos")]
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
    /// The external (ACP) agent: settings, Test, turns, its processes.
    pub acp: AcpService,
    /// App log directory, for the diagnostics bundle. `None` if the OS gave none.
    pub log_dir: Option<std::path::PathBuf>,
    /// Set once the user chose to quit with transfers still running.
    pub quit_confirmed: AtomicBool,
    /// When a quit was last held. A second quit soon after goes through, so
    /// the app can always be quit even if the window cannot answer.
    pub quit_held_at: std::sync::Mutex<Option<std::time::Instant>>,
}

/// A second quit within this long of a held one is not held again.
const QUIT_AGAIN: std::time::Duration = std::time::Duration::from_secs(5);

/// Window event: a quit or close was held because exports or imports run.
const QUIT_REQUESTED: &str = "quit-requested";

/// Window event: the approval dock's queue changed (no payload; it re-reads).
const TOOL_APPROVALS_CHANGED: &str = "tool-approvals-changed";

/// True when closing now would cut off a running export or import. The
/// window is told and asks the user; `confirm_quit` quits after that.
fn hold_quit(app: &tauri::AppHandle) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    if state.quit_confirmed.load(Ordering::SeqCst) {
        return false;
    }
    let running = state.services.connections().running_transfers();
    if running == 0 {
        return false;
    }
    if let Ok(mut held) = state.quit_held_at.lock() {
        if held.is_some_and(|at| at.elapsed() < QUIT_AGAIN) {
            tracing::info!(running, "quit asked twice: cancelling transfers");
            state.quit_confirmed.store(true, Ordering::SeqCst);
            state.services.connections().cancel_transfers();
            return false;
        }
        *held = Some(std::time::Instant::now());
    }
    tracing::info!(running, "quit held: transfers are running");
    let _ = app.emit(QUIT_REQUESTED, running);
    true
}

/// The user chose to quit while transfers run: cancel them, then exit.
/// Partial exports are removed (they are staged), imported batches stay.
#[tauri::command]
fn confirm_quit(app: tauri::AppHandle, state: tauri::State<'_, AppState>) {
    state.quit_confirmed.store(true, Ordering::SeqCst);
    state.services.connections().cancel_transfers();
    app.exit(0);
}

/// Native menu. Stays sync: no keychain and no SQLite. See `ipc/mod.rs`.
#[cfg(target_os = "macos")]
#[tauri::command]
fn set_native_menu(app: tauri::AppHandle, state: menu::NativeMenuState) -> ipc::IpcResult<()> {
    crate::ipc::guard_sync(|| {
        menu::apply_menu(&app, &state).map_err(|e| ipc::IpcError::internal(e.to_string()))
    })
}

/// Windows and Linux have no native menu: the custom titlebar draws it.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn set_native_menu() -> ipc::IpcResult<()> {
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(|app| menu::build_menu(app, &menu::NativeMenuState::default()))
        .on_menu_event(|app, event| menu::handle_menu_event(app, event.id().as_ref()));
    builder
        // No JS import: its injected click handler opens `target="_blank"`
        // http(s)/mailto links (agent chat markdown) in the default browser.
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        // Updates from GitHub Releases (`latest.json`), driven by
        // `ipc::updates` so requests get a connect timeout.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(ipc::updates::PendingUpdate::default())
        // `relaunch` after an update is installed.
        .plugin(tauri_plugin_process::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && hold_quit(window.app_handle()) {
                    api.prevent_close();
                }
            }
        })
        .setup(|app| {
            let log_dir = app.path().app_log_dir().ok();
            if let Some(dir) = &log_dir {
                logging::init(dir);
            }
            tracing::info!(
                version = %app.package_info().version,
                os = std::env::consts::OS,
                arch = std::env::consts::ARCH,
                "mystique starting"
            );
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("app data dir: {e}"))?;
            std::fs::create_dir_all(&data_dir)?;

            let db_path = data_dir.join(mystique_core::storage::sqlite::db_file_name());
            let db = AppDatabase::open(&db_path).map_err(|e| e.to_string())?;
            let services = Arc::new(AppServices::new(db, &data_dir));
            // The approval dock refreshes when an agent or MCP write waits or ends.
            let handle = app.handle().clone();
            services.tool_approvals().set_notifier(move || {
                let _ = handle.emit(TOOL_APPROVALS_CHANGED, ());
            });
            // Setup never touches the keychain. The vault key loads on first secret use.
            let mcp = McpService::new(Arc::clone(&services));
            let agent = AgentService::new(Arc::clone(&services));
            // External ACP agents: run in an empty folder under app data.
            // Agents without HTTP MCP start this binary as the stdio bridge.
            let mut acp = AcpService::new(Arc::clone(&services), &data_dir);
            if let Ok(exe) = std::env::current_exe() {
                acp = acp.with_bridge(exe);
            }
            let boot = mcp.clone();
            tauri::async_runtime::spawn(async move {
                boot.restore_if_enabled().await;
            });

            app.manage(AppState {
                services,
                mcp,
                agent,
                acp,
                log_dir,
                quit_confirmed: AtomicBool::new(false),
                quit_held_at: std::sync::Mutex::new(None),
            });
            apply_app_icon(app).map_err(|e| format!("app icon: {e}"))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // engines / connection pools
            ipc::engines::parse_engine_connection_url,
            ipc::engines::format_engine_connection_url,
            ipc::engines::test_engine_connection,
            ipc::engines::trust_ssh_host_key,
            ipc::engines::open_engine_session,
            ipc::engines::close_engine_session,
            ipc::engines::ping_engine_session,
            ipc::engines::list_engine_sessions,
            ipc::engines::get_engine_session,
            ipc::engines::unlock_engine_writes,
            ipc::engines::start_timed_write_unlock,
            ipc::engines::end_timed_write_unlock,
            ipc::engines::timed_write_unlock_status,
            ipc::engines::preview_write_rows,
            ipc::engines::explain_engine_query,
            ipc::engines::execute_engine_batch,
            ipc::engines::execute_pinned_batch,
            ipc::engines::pin_query_tab,
            ipc::engines::unpin_query_tab,
            ipc::engines::commit_pinned_tab,
            ipc::engines::rollback_pinned_tab,
            ipc::engines::pinned_tab_status,
            ipc::engines::cancel_engine_query,
            ipc::engines::describe_engine_table,
            ipc::engines::list_engine_catalog_columns,
            ipc::engines::apply_engine_table_edits,
            ipc::engines::rollback_engine_table_edits,
            ipc::engines::distinct_engine_column_values,
            ipc::engines::fetch_engine_large_value,
            ipc::engines::fetch_engine_table_preview,
            ipc::engines::fetch_engine_table_preview_stats,
            ipc::engines::list_engine_catalog,
            ipc::engines::list_engine_catalog_children,
            ipc::engines::list_engine_relationships,
            ipc::engines::apply_engine_table_admin,
            ipc::engines::export_engine_object,
            ipc::engines::preview_engine_import,
            ipc::engines::import_engine_object,
            ipc::engines::running_engine_transfers,
            ipc::engines::invalidate_engine_metadata,
            confirm_quit,
            ipc::engines::describe_engine_object_structure,
            ipc::engines::describe_engine_table_definition,
            ipc::engines::list_engine_table_definitions,
            ipc::engines::plan_engine_ddl,
            ipc::engines::apply_engine_ddl,
            ipc::engines::plan_engine_schema_diff,
            ipc::engines::apply_engine_schema_diff,
            ipc::engines::fetch_engine_server_metrics,
            ipc::engines::terminate_engine_session,
            // profiles
            ipc::profiles::list_profiles,
            ipc::profiles::save_profile,
            ipc::profiles::delete_profile,
            ipc::profiles::touch_profile_connected,
            // query library (per-connection, no groups)
            ipc::query_library::list_saved_queries,
            ipc::query_library::save_query,
            ipc::query_library::delete_saved_query,
            // workspace tabs (restored on reconnect)
            ipc::grid_layout::get_grid_column_layout,
            ipc::grid_layout::save_grid_column_layout,
            ipc::grid_layout::latest_grid_save,
            ipc::workspace_tabs::list_workspace_tabs,
            ipc::workspace_tabs::replace_workspace_tabs,
            ipc::workspace_tabs::upsert_workspace_tab,
            // history
            ipc::history::append_query_history,
            ipc::history::list_query_history,
            ipc::history::list_recent_query_history,
            // settings
            set_native_menu,
            ipc::settings::get_vault_status,
            ipc::settings::retry_vault_unlock,
            ipc::settings::purge_keychain_items,
            ipc::diagnostics::export_diagnostics,
            // app updates (GitHub Releases)
            ipc::updates::check_for_update,
            ipc::updates::download_update,
            ipc::updates::install_update,
            // MCP (external clients). In-app agents must call AgentTools, not HTTP.
            ipc::mcp::get_mcp_status,
            ipc::mcp::set_mcp_enabled,
            ipc::mcp::rotate_mcp_token,
            ipc::mcp::list_mcp_harnesses,
            ipc::mcp::install_mcp_harness,
            // MCP and agent access levels, and the approval dock.
            ipc::mcp::get_tool_access,
            ipc::mcp::set_tool_access_default,
            ipc::mcp::set_tool_access_level,
            ipc::mcp::set_tool_access_ask,
            ipc::mcp::list_tool_approvals,
            ipc::mcp::decide_tool_approval,
            // in-app query-tab agent
            ipc::agent::get_ai_settings,
            ipc::agent::set_ai_enabled,
            ipc::agent::set_ai_ollama_url,
            ipc::agent::set_ai_api_key,
            ipc::agent::list_agent_models,
            ipc::agent::list_agent_chats,
            ipc::agent::delete_agent_chat,
            ipc::agent::list_agent_messages,
            ipc::agent::update_agent_chat,
            ipc::agent::send_agent_message,
            ipc::agent::cancel_agent_message,
            // external (ACP) agent in the same pane
            ipc::agent::get_acp_settings,
            ipc::agent::save_acp_agent,
            ipc::agent::remove_acp_agent,
            ipc::agent::set_default_agent,
            ipc::agent::detect_acp_agents,
            ipc::agent::add_detected_acp_agent,
            ipc::agent::test_acp_agent,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                if hold_quit(app) {
                    api.prevent_exit();
                }
                return;
            }
            if let tauri::RunEvent::Exit = event {
                tracing::info!("mystique exiting");
                let Some(state) = app.try_state::<AppState>() else {
                    return;
                };
                // No external agent outlives the app: stop the runs, then
                // kill their process groups (async cleanup may not run now).
                state.agent.runtime().cancel_all();
                let killed = state.acp.kill_all();
                if killed > 0 {
                    tracing::info!(killed, "stopped external agents");
                }
                let services = std::sync::Arc::clone(&state.services);
                services.forget_session_passwords();
                tauri::async_runtime::block_on(async move {
                    services.connections().rollback_all_pins().await;
                });
            }
        });
}

#[cfg(test)]
mod updater_config_tests {
    use base64::Engine;

    /// The updater reads `plugins.updater` when the app starts; a bad value
    /// would only show up then. Parse it the way the plugin does.
    #[test]
    fn the_updater_config_parses_and_points_at_github() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let updater: tauri_plugin_updater::Config =
            serde_json::from_value(conf["plugins"]["updater"].clone()).expect("updater config");
        let urls: Vec<String> = updater.endpoints.iter().map(|u| u.to_string()).collect();
        assert_eq!(
            urls,
            ["https://github.com/yash1200/mystique/releases/latest/download/latest.json"]
        );
        let key = base64::engine::general_purpose::STANDARD
            .decode(updater.pubkey.trim())
            .expect("pubkey is base64");
        let key = String::from_utf8(key).expect("pubkey text");
        assert!(
            key.starts_with("untrusted comment: minisign public key"),
            "{key}"
        );
    }

    /// Plain builds need no key; only the release overlay asks for the archive.
    #[test]
    fn only_the_release_config_builds_update_archives() {
        let base: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        assert!(base["bundle"].get("createUpdaterArtifacts").is_none());
        let release: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.release.conf.json")).expect("release conf");
        assert_eq!(release["bundle"]["createUpdaterArtifacts"], true);
    }
}
