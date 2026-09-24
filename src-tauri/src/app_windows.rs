//! Regular (non-overlay) windows. Phase 1 has only the settings window.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Theme, WebviewUrl, WebviewWindowBuilder};
use ts_rs::TS;

use crate::settings::schema::ThemePreference;
use crate::state::AppState;

pub const SETTINGS_LABEL: &str = "settings";
pub const NAVIGATE_EVENT: &str = "settings://navigate";

/// Opens (or focuses) settings, optionally jumping to a section id.
pub fn open_settings(app: &AppHandle, section: Option<&str>) {
    if let Some(w) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(s) = section {
            let _ = w.emit(NAVIGATE_EVENT, s);
        }
        return;
    }
    let url = match section {
        Some(s) => format!("index.html#{s}"),
        None => "index.html".into(),
    };
    let built = WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App(url.into()))
        .title("Helpy Settings")
        .inner_size(1040.0, 720.0)
        .min_inner_size(760.0, 520.0)
        .center()
        .theme(native_theme(app))
        .visible(true)
        .build();
    match built {
        Ok(w) => {
            let _ = w.set_focus();
        }
        Err(e) => log::error!("could not open settings: {e}"),
    }
}

fn native_theme(app: &AppHandle) -> Option<Theme> {
    match app.state::<AppState>().settings().general.theme {
        ThemePreference::System => None,
        ThemePreference::Light => Some(Theme::Light),
        ThemePreference::Dark => Some(Theme::Dark),
    }
}

/// Keeps native title bars in step with the theme setting.
pub fn apply_theme(app: &AppHandle) {
    let theme = native_theme(app);
    for (label, w) in app.webview_windows() {
        if !crate::overlay::is_overlay(&label) {
            let _ = w.set_theme(theme);
        }
    }
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlatformInfo {
    /// "windows", "macos", or "linux".
    pub os: String,
    pub wayland: bool,
    pub version: String,
}

#[tauri::command]
pub fn platform_info(app: AppHandle) -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS.into(),
        wayland: cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some(),
        version: app.package_info().version.to_string(),
    }
}
