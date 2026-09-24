//! Platform facts the settings page shows, and Linux session setup.

use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlatformInfo {
    /// "windows", "macos" or "linux".
    pub os: String,
    /// Features that don't work, or work partly, in this session.
    pub limitations: Vec<String>,
}

/// On a Wayland session with XWayland available, run through XWayland: it
/// gives Helpy a global cursor position, always-on-top and click-through that
/// native Wayland doesn't allow. Must run before Tauri starts GTK.
#[cfg(target_os = "linux")]
pub fn prefer_x11() {
    let wayland = std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland");
    if wayland && std::env::var_os("GDK_BACKEND").is_none() && std::env::var_os("DISPLAY").is_some()
    {
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

/// A compositing manager owns the `_NET_WM_CM_S<screen>` selection.
#[cfg(target_os = "linux")]
fn x11_has_compositor() -> bool {
    use x11rb::protocol::xproto::ConnectionExt;
    let check = || -> Option<bool> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let atom = conn
            .intern_atom(false, format!("_NET_WM_CM_S{screen}").as_bytes())
            .ok()?
            .reply()
            .ok()?
            .atom;
        let owner = conn.get_selection_owner(atom).ok()?.reply().ok()?.owner;
        Some(owner != 0)
    };
    // If we can't ask, don't warn about something we can't confirm.
    check().unwrap_or(true)
}

#[cfg(not(target_os = "linux"))]
fn x11_has_compositor() -> bool {
    true
}

#[tauri::command]
pub fn platform_info() -> PlatformInfo {
    let mut limitations = Vec::new();
    if cfg!(target_os = "linux") {
        let wayland = std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland");
        let x11 = std::env::var("GDK_BACKEND").is_ok_and(|v| v == "x11") || !wayland;
        if wayland && x11 {
            limitations.extend([
                "You're on Wayland, so Helpy runs through XWayland.".into(),
                "The buddy only follows the cursor while it's over apps that also run through XWayland.".into(),
                "Global hotkeys only work while an XWayland app is focused.".into(),
                "Fullscreen auto-hide can't see native Wayland apps.".into(),
            ]);
        } else if wayland {
            limitations.extend([
                "XWayland isn't available, so the buddy can't follow the cursor.".into(),
                "Global hotkeys and fullscreen auto-hide aren't available.".into(),
                "Overlays may not stay on top or let clicks through.".into(),
            ]);
        } else if !x11_has_compositor() {
            limitations.push(
                "No compositor is running, so overlays can't be transparent. Turn on compositing in your window manager.".into(),
            );
        }
    }
    PlatformInfo {
        os: std::env::consts::OS.into(),
        limitations,
    }
}
