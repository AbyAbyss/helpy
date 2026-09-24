//! Helpy's own app windows (not the overlays).

use tauri::{AppHandle, Manager};

pub const SETTINGS: &str = "settings";

pub fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}
