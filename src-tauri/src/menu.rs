//! Native app menu (macOS only). Rebuilt when the frontend reports Home vs
//! Workspace. Windows and Linux draw their menu in the custom titlebar
//! (`src/lib/features/titlebar/menu.ts`), which mirrors this one.

use serde::Deserialize;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Emitter, Manager, Wry,
};

use crate::APP_DISPLAY_NAME;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuPage {
    #[default]
    Home,
    Workspace,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeMenuState {
    #[serde(default)]
    pub page: MenuPage,
    #[serde(default)]
    pub has_selection: bool,
    #[serde(default)]
    pub has_session: bool,
    #[serde(default)]
    pub has_tab: bool,
    #[serde(default)]
    pub has_closed_tab: bool,
    /// Home: the new / edit connection form is on screen.
    #[serde(default)]
    pub has_form: bool,
}

pub fn apply_menu(app: &AppHandle, state: &NativeMenuState) -> tauri::Result<()> {
    let menu = build_menu(app, state)?;
    app.set_menu(menu)?;
    Ok(())
}

pub fn build_menu(app: &AppHandle, state: &NativeMenuState) -> tauri::Result<Menu<Wry>> {
    let file_menu = match state.page {
        MenuPage::Home => file_menu_home(app)?,
        MenuPage::Workspace => file_menu_workspace(app, state)?,
    };
    let connections_menu = match state.page {
        MenuPage::Home => connections_menu_home(app, state)?,
        MenuPage::Workspace => connections_menu_workspace(app, state)?,
    };
    let edit_menu = edit_menu(app)?;
    let view_menu = view_menu(app, state.page)?;
    let window_menu = window_menu(app)?;
    let app_menu = app_menu(app)?;
    let help_menu = help_menu(app)?;
    Menu::with_items(
        app,
        &[
            &app_menu,
            &file_menu,
            &connections_menu,
            &edit_menu,
            &view_menu,
            &window_menu,
            &help_menu,
        ],
    )
}

fn item(
    app: &AppHandle,
    id: &str,
    title: &str,
    enabled: bool,
    accelerator: Option<&str>,
) -> tauri::Result<MenuItem<Wry>> {
    MenuItem::with_id(app, id, title, enabled, accelerator)
}

fn close_window_item(app: &AppHandle) -> tauri::Result<MenuItem<Wry>> {
    item(
        app,
        "window_close",
        "Close Window",
        true,
        Some("Shift+CmdOrCtrl+W"),
    )
}

fn file_menu_home(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    let close_window = close_window_item(app)?;
    Submenu::with_items(app, "File", true, &[&close_window])
}

fn file_menu_workspace(app: &AppHandle, state: &NativeMenuState) -> tauri::Result<Submenu<Wry>> {
    let session = state.has_session;
    let tab = state.has_tab;
    let new_query = item(
        app,
        "workspace_new_query",
        "New Query",
        session,
        Some("CmdOrCtrl+T"),
    )?;
    let save_query = item(
        app,
        "workspace_save_query",
        "Save",
        tab,
        Some("CmdOrCtrl+S"),
    )?;
    let close_tab = item(
        app,
        "workspace_close_tab",
        "Close Tab",
        tab,
        Some("CmdOrCtrl+W"),
    )?;
    let reopen_tab = item(
        app,
        "workspace_reopen_tab",
        "Reopen Closed Tab",
        state.has_closed_tab,
        Some("Shift+CmdOrCtrl+T"),
    )?;
    let close_window = close_window_item(app)?;
    Submenu::with_items(
        app,
        "File",
        true,
        &[
            &new_query,
            &save_query,
            &close_tab,
            &reopen_tab,
            &PredefinedMenuItem::separator(app)?,
            &close_window,
        ],
    )
}

fn connections_menu_home(app: &AppHandle, state: &NativeMenuState) -> tauri::Result<Submenu<Wry>> {
    let sel = state.has_selection;
    let new_conn = item(app, "home_new_connection", "New Connection", true, None)?;
    let import_url = item(app, "home_import_url", "Import Connection URL…", true, None)?;
    let save = item(
        app,
        "home_save",
        "Save Connection",
        state.has_form,
        Some("CmdOrCtrl+S"),
    )?;
    let connect = item(app, "home_connect", "Connect", sel, None)?;
    let edit = item(app, "home_edit", "Edit Connection", sel, None)?;
    let duplicate = item(app, "home_duplicate", "Duplicate Connection", sel, None)?;
    let copy_url = item(app, "home_copy_url", "Copy Connection URL", sel, None)?;
    let delete = item(app, "home_delete", "Delete Connection", sel, None)?;
    let refresh = item(app, "home_refresh", "Refresh Connections", true, None)?;
    Submenu::with_items(
        app,
        "Connections",
        true,
        &[
            &new_conn,
            &import_url,
            &save,
            &PredefinedMenuItem::separator(app)?,
            &connect,
            &edit,
            &duplicate,
            &copy_url,
            &delete,
            &PredefinedMenuItem::separator(app)?,
            &refresh,
        ],
    )
}

fn connections_menu_workspace(
    app: &AppHandle,
    state: &NativeMenuState,
) -> tauri::Result<Submenu<Wry>> {
    let session = state.has_session;
    let open_conn = item(
        app,
        "workspace_open_connection",
        "Open Connection…",
        true,
        Some("CmdOrCtrl+O"),
    )?;
    let details = item(
        app,
        "workspace_connection_details",
        "Connection Details",
        session,
        None,
    )?;
    let copy_url = item(
        app,
        "workspace_copy_url",
        "Copy Connection URL",
        session,
        None,
    )?;
    let refresh = item(
        app,
        "workspace_refresh_catalog",
        "Refresh Catalog",
        session,
        None,
    )?;
    let disconnect = item(app, "workspace_disconnect", "Disconnect", session, None)?;
    Submenu::with_items(
        app,
        "Connections",
        true,
        &[
            &open_conn,
            &details,
            &copy_url,
            &refresh,
            &PredefinedMenuItem::separator(app)?,
            &disconnect,
        ],
    )
}

fn edit_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    Submenu::with_items(
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
    )
}

fn view_menu(app: &AppHandle, page: MenuPage) -> tauri::Result<Submenu<Wry>> {
    let zoom_in = item(app, "view_zoom_in", "Zoom In", true, Some("CmdOrCtrl+="))?;
    let zoom_out = item(app, "view_zoom_out", "Zoom Out", true, Some("CmdOrCtrl+-"))?;
    let zoom_reset = item(
        app,
        "view_zoom_reset",
        "Actual Size",
        true,
        Some("CmdOrCtrl+0"),
    )?;

    if page == MenuPage::Workspace {
        let explorer = item(app, "workspace_view_explorer", "Explorer", true, None)?;
        let saved = item(app, "workspace_view_saved", "Saved Queries", true, None)?;
        let history = item(app, "workspace_view_history", "History", true, None)?;
        let go_home = item(app, "workspace_go_home", "Go to Home", true, None)?;
        return Submenu::with_items(
            app,
            "View",
            true,
            &[
                &explorer,
                &saved,
                &history,
                &PredefinedMenuItem::separator(app)?,
                &go_home,
                &PredefinedMenuItem::separator(app)?,
                &zoom_in,
                &zoom_out,
                &zoom_reset,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::fullscreen(app, None)?,
            ],
        );
    }

    Submenu::with_items(
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
    )
}

fn window_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::bring_all_to_front(app, None)?,
        ],
    )
}

fn help_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    let shortcuts = item(app, "help_shortcuts", "Keyboard Shortcuts", true, None)?;
    Submenu::with_items(app, "Help", true, &[&shortcuts])
}

fn app_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    // About opens the app's own dialog (`app.about`), not the OS panel, so it
    // follows the theme and carries the update status.
    let about = item(app, "app_about", "About Mystique", true, None)?;
    let updates = item(app, "app_check_updates", "Check for Updates…", true, None)?;
    let settings = item(app, "app_settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let shortcuts = item(app, "app_shortcuts", "Keyboard Shortcuts", true, None)?;

    Submenu::with_items(
        app,
        APP_DISPLAY_NAME,
        true,
        &[
            &about,
            &updates,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &shortcuts,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some("Hide Mystique"))?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some("Quit Mystique"))?,
        ],
    )
}

/// The one event every frontend menu item sends; the payload is the item id.
/// The frontend runs the command bound to that id (`src/lib/core/commands`).
const MENU_COMMAND_EVENT: &str = "menu-command";

pub fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "window_close" => close_focused_window(app),
        _ => emit_menu_command(app, id),
    }
}

/// `AppHandle::emit` already reaches every webview. Emitting per window too
/// delivered each menu command more than once.
fn emit_menu_command(app: &AppHandle, id: &str) {
    let _ = app.emit(MENU_COMMAND_EVENT, id);
}

fn close_focused_window(app: &AppHandle) {
    if let Some(win) = app
        .webview_windows()
        .into_iter()
        .find_map(|(_label, w)| w.is_focused().ok().filter(|&f| f).map(|_| w))
    {
        let _ = win.close();
        return;
    }
    if let Some((_label, win)) = app.webview_windows().into_iter().next() {
        let _ = win.close();
    }
}
