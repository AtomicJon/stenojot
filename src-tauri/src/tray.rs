//! System tray icon and menu for controlling recording and navigating the app.
//!
//! Creates a tray icon with a right-click context menu that reflects the current
//! recording state. Left-clicking the icon toggles the main window. The menu is
//! rebuilt via `refresh_tray_menu` whenever recording state changes so the
//! available actions stay in sync.

use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::AppState;

/// Menu item IDs used for matching tray menu events.
const ID_START: &str = "start_recording";
const ID_PAUSE: &str = "pause_recording";
const ID_RESUME: &str = "resume_recording";
const ID_STOP: &str = "stop_recording";
const ID_CURRENT_SESSION: &str = "current_session";
const ID_MEETINGS: &str = "meetings";
const ID_SETTINGS: &str = "settings";
const ID_QUIT: &str = "quit";

/// Create the system tray icon and wire up its event handlers. Called once
/// during app setup. Uses the app's default window icon (from `bundle.icon`
/// in `tauri.conf.json`) so the tray matches the window titlebar.
pub fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let menu = build_idle_menu(app)?;

    let icon = app
        .default_window_icon()
        .ok_or("No default window icon configured — add icons/icon.png to bundle.icon in tauri.conf.json")?
        .clone();

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("StenoJot")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            handle_menu_event(app, event.id.as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// Build the initial (idle / not-recording) tray menu.
fn build_idle_menu(app: &tauri::App) -> Result<Menu<tauri::Wry>, tauri::Error> {
    let start = MenuItem::with_id(app, ID_START, "Start Recording", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let session = MenuItem::with_id(app, ID_CURRENT_SESSION, "Current Session", true, None::<&str>)?;
    let meetings = MenuItem::with_id(app, ID_MEETINGS, "Meetings", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, ID_SETTINGS, "Settings", true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, ID_QUIT, "Quit", true, None::<&str>)?;

    Menu::with_items(app, &[&start, &sep1, &session, &meetings, &settings, &sep2, &quit])
}

/// Build the tray menu based on current recording state.
fn build_menu(
    app: &AppHandle,
    is_recording: bool,
    is_paused: bool,
) -> Result<Menu<tauri::Wry>, tauri::Error> {
    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();

    if is_recording {
        if is_paused {
            items.push(Box::new(MenuItem::with_id(app, ID_RESUME, "Resume Recording", true, None::<&str>)?));
        } else {
            items.push(Box::new(MenuItem::with_id(app, ID_PAUSE, "Pause Recording", true, None::<&str>)?));
        }
        items.push(Box::new(MenuItem::with_id(app, ID_STOP, "Stop Recording", true, None::<&str>)?));
    } else {
        items.push(Box::new(MenuItem::with_id(app, ID_START, "Start Recording", true, None::<&str>)?));
    }

    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(app, ID_CURRENT_SESSION, "Current Session", true, None::<&str>)?));
    items.push(Box::new(MenuItem::with_id(app, ID_MEETINGS, "Meetings", true, None::<&str>)?));
    items.push(Box::new(MenuItem::with_id(app, ID_SETTINGS, "Settings", true, None::<&str>)?));
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(app, ID_QUIT, "Quit", true, None::<&str>)?));

    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        items.iter().map(|b| b.as_ref()).collect();
    Menu::with_items(app, &refs)
}

/// Handles a tray menu item click by dispatching to the appropriate action.
fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        ID_START => {
            show_and_navigate(app, "/");
            let _ = app.emit("tray-start-recording", ());
        }
        ID_PAUSE => {
            let _ = app.emit("tray-pause-recording", ());
        }
        ID_RESUME => {
            let _ = app.emit("tray-resume-recording", ());
        }
        ID_STOP => {
            let _ = app.emit("tray-stop-recording", ());
        }
        ID_CURRENT_SESSION => {
            show_and_navigate(app, "/");
        }
        ID_MEETINGS => {
            show_and_navigate(app, "/meetings");
        }
        ID_SETTINGS => {
            show_and_navigate(app, "/settings");
        }
        ID_QUIT => {
            app.exit(0);
        }
        _ => {}
    }
}

/// Shows the main window and tells the frontend to navigate to the given route.
fn show_and_navigate(app: &AppHandle, route: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    let _ = app.emit("tray-navigate", route);
}

/// Show the main window if hidden, or hide it if visible.
fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }
}

/// Refreshes the tray menu to reflect the current recording state.
///
/// Call this whenever recording state changes (start, stop, pause, resume)
/// so the tray menu stays in sync.
pub fn refresh_tray_menu(app: &AppHandle) {
    let (is_recording, is_paused) = {
        let state = app.state::<Mutex<AppState>>();
        let app_state = state.lock().expect("Failed to lock AppState for tray refresh");
        (app_state.is_recording, app_state.is_paused)
    };

    if let Ok(menu) = build_menu(app, is_recording, is_paused) {
        if let Some(tray) = app.tray_by_id("main-tray") {
            let _ = tray.set_menu(Some(menu));
        }
    }
}
