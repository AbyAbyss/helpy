//! The OS permissions Helpy needs, for the welcome tour, settings and the
//! check at startup.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use crate::settings::SettingsStore;

/// Where one permission stands.
#[derive(Serialize, TS, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Access {
    Allowed,
    Denied,
    /// The system hasn't asked yet; it will the first time it's needed.
    NotAsked,
}

/// What the OS lets Helpy do. None where this system has no such permission.
#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Permissions {
    /// Hearing the voice shortcut.
    pub microphone: Option<Access>,
    /// Seeing the screen (macOS).
    pub screen: Option<Access>,
    /// Reading other apps' controls, clicking for the user and moving
    /// windows (macOS).
    pub accessibility: Option<Access>,
}

impl Permissions {
    /// The ones turned off, by name. Ones never asked for aren't: the
    /// system asks the first time they're needed.
    pub fn missing(&self) -> Vec<&'static str> {
        [
            ("Microphone", self.microphone),
            ("Screen Recording", self.screen),
            ("Accessibility", self.accessibility),
        ]
        .into_iter()
        .filter(|(_, a)| *a == Some(Access::Denied))
        .map(|(name, _)| name)
        .collect()
    }
}

fn allowed(yes: bool) -> Access {
    if yes {
        Access::Allowed
    } else {
        Access::Denied
    }
}

#[cfg(target_os = "macos")]
mod os {
    use super::Access;
    use objc2::runtime::AnyClass;
    use objc2_foundation::NSString;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {
        static AVMediaTypeAudio: &'static NSString;
    }

    pub fn microphone() -> Option<Access> {
        let class = AnyClass::get(c"AVCaptureDevice")?;
        // AVAuthorizationStatus: 0 not determined, 1 restricted, 2 denied,
        // 3 authorized.
        let status: isize =
            unsafe { objc2::msg_send![class, authorizationStatusForMediaType: AVMediaTypeAudio] };
        Some(match status {
            3 => Access::Allowed,
            0 => Access::NotAsked,
            _ => Access::Denied,
        })
    }

    pub fn screen() -> Option<Access> {
        Some(super::allowed(unsafe { CGPreflightScreenCaptureAccess() }))
    }

    /// Shows macOS's prompt; it only does while Helpy has no answer on record.
    pub fn request_screen() -> bool {
        unsafe { CGRequestScreenCaptureAccess() }
    }
}

#[cfg(target_os = "windows")]
mod os {
    use super::Access;

    /// Windows' microphone switches: for every app, then for desktop apps.
    pub fn microphone() -> Option<Access> {
        const STORE: &str =
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
        let denied = |key: &str| {
            std::process::Command::new("reg")
                .args(["query", key, "/v", "Value"])
                .output()
                .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("Deny"))
        };
        Some(super::allowed(!denied(STORE) && !denied(&format!(r"{STORE}\NonPackaged"))))
    }

    pub fn screen() -> Option<Access> {
        None
    }

    pub fn request_screen() -> bool {
        true
    }
}

#[cfg(target_os = "linux")]
mod os {
    use super::Access;

    pub fn microphone() -> Option<Access> {
        None
    }

    pub fn screen() -> Option<Access> {
        None
    }

    pub fn request_screen() -> bool {
        true
    }
}

#[tauri::command]
pub fn permissions_status() -> Permissions {
    Permissions {
        microphone: os::microphone(),
        screen: os::screen(),
        accessibility: crate::a11y::trusted().map(allowed),
    }
}

/// Asks for "microphone", "screen" or "accessibility": the system's own
/// prompt when it still shows one, otherwise its settings page.
#[tauri::command]
pub async fn permissions_request(which: String) -> Result<Permissions, String> {
    let now = permissions_status();
    match which.as_str() {
        "microphone" if now.microphone == Some(Access::NotAsked) => {
            // Opening the microphone is what makes macOS ask.
            tauri::async_runtime::spawn_blocking(|| {
                let _mic = crate::voice::audio::start(None, false);
                std::thread::sleep(std::time::Duration::from_millis(300));
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        "screen" if now.screen != Some(Access::Allowed) => {
            if !os::request_screen() {
                permissions_open(which)?;
            }
        }
        "accessibility" if now.accessibility != Some(Access::Allowed) => {
            crate::a11y::request_trust();
            permissions_open(which)?;
        }
        _ => permissions_open(which)?,
    }
    Ok(permissions_status())
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
        ("windows", "microphone") => "ms-settings:privacy-microphone",
        _ => return Err("There's no such settings page on this system".into()),
    };
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Quits and starts Helpy again: macOS applies Screen Recording only then.
#[tauri::command]
pub fn permissions_relaunch(app: AppHandle) {
    app.restart();
}

/// At startup, after the welcome tour: when something Helpy needs isn't
/// allowed, open Settings on the page that shows it.
pub fn check_at_startup(app: &AppHandle) {
    let s = app.state::<SettingsStore>().get();
    if !s.general.onboarded || !s.general.check_permissions {
        return;
    }
    let missing = permissions_status().missing();
    if missing.is_empty() {
        return;
    }
    log::info!("missing permissions at startup: {}", missing.join(", "));
    crate::windows::show_settings(app);
    // The page asks for its section when it loads; this covers one that
    // already has.
    let _ = app.emit_to(crate::windows::SETTINGS, crate::windows::OPEN_SECTION_EVENT, "permissions");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_permissions_turned_off_are_missing() {
        let p = Permissions {
            microphone: Some(Access::Denied),
            screen: Some(Access::Allowed),
            accessibility: None,
        };
        assert_eq!(p.missing(), ["Microphone"]);
        let p = Permissions {
            microphone: Some(Access::NotAsked),
            screen: Some(Access::Denied),
            accessibility: Some(Access::Denied),
        };
        assert_eq!(p.missing(), ["Screen Recording", "Accessibility"]);
    }
}

#[cfg(test)]
mod live {
    /// Prints this computer's permissions: `cargo test live_status -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_status() {
        println!("{:?}", super::permissions_status());
    }
}
