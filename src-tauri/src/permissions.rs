//! The OS permissions Helpy needs, for the welcome tour and settings.

use serde::Serialize;
use ts_rs::TS;

/// What the OS lets Helpy do. None where no permission is needed.
#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Permissions {
    /// Reading other apps' controls and clicking for the user (macOS).
    pub accessibility: Option<bool>,
}

#[tauri::command]
pub fn permissions_status() -> Permissions {
    Permissions {
        accessibility: crate::a11y::trusted(),
    }
}

/// Asks macOS to add Helpy to the Accessibility list (it shows its own
/// prompt).
#[tauri::command]
pub fn permissions_request() -> Permissions {
    crate::a11y::request_trust();
    permissions_status()
}

/// Opens the system settings page for "microphone", "screen" or
/// "accessibility".
#[tauri::command]
pub fn permissions_open(which: String) -> Result<(), String> {
    let url = match (std::env::consts::OS, which.as_str()) {
        ("macos", "microphone") => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        ("macos", "screen") => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        ("macos", "accessibility") => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        _ => return Err("There's no such settings page on this system".into()),
    };
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}
