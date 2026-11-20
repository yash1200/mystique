//! Background mode. With the MCP server on, closing the window hides it
//! instead of quitting, so MCP clients keep working. A tray (menu bar) icon
//! shows until the app quits: it reopens the window or quits.

use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use crate::AppState;

const TRAY_ID: &str = "background";

/// Build the tray icon. It stays until the app quits.
pub fn create(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "tray_open", "Open Mystique", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray_quit", "Quit Mystique", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &quit])?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(crate::APP_ICON_PNG)?)
        .tooltip("Mystique")
        .menu(&menu)
        // macOS opens the menu on click. Windows opens the window on click
        // and keeps the menu on right click.
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_open" => show_window(app),
            // Goes through `ExitRequested`, so running transfers still ask first.
            "tray_quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if !cfg!(target_os = "macos") {
                    show_window(tray.app_handle());
                }
            }
        })
        .build(app)?;
    Ok(())
}

/// True when closing the main window should hide it rather than quit.
pub fn keeps_running(app: &AppHandle) -> bool {
    app.try_state::<AppState>()
        .is_some_and(|state| state.mcp.enabled_setting())
}

/// Bring the main window back (tray click, Dock click, or a held quit that
/// needs the window to ask).
pub fn show_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}
