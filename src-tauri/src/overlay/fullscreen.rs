//! Detects when a fullscreen app (game, video, slideshow) has focus so the buddy and
//! floating cards can step aside.
//!
//! Windows: compares the foreground window's rect with its monitor's rect, and also asks
//! the shell (`SHQueryUserNotificationState`) which catches presentation mode and D3D
//! exclusive fullscreen. macOS and X11 are planned for Phase 9 and report "not fullscreen".

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

pub const FULLSCREEN_EVENT: &str = "fullscreen://changed";

#[derive(Serialize, Clone, Debug, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FullscreenState {
    pub active: bool,
    /// Physical rect of the monitor the fullscreen app covers: x, y, width, height.
    pub monitor: Option<[i32; 4]>,
}

pub fn spawn(app: AppHandle) {
    std::thread::Builder::new()
        .name("helpy-fullscreen".into())
        .spawn(move || {
            let mut last = FullscreenState { active: false, monitor: None };
            loop {
                let now = detect();
                if now != last {
                    let _ = app.emit(FULLSCREEN_EVENT, &now);
                    last = now;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        })
        .expect("spawn fullscreen thread");
}

#[cfg(windows)]
fn detect() -> FullscreenState {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
    };

    let none = FullscreenState { active: false, monitor: None };
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.0.is_null() {
            return none;
        }
        // Ignore Helpy's own windows and the desktop/shell.
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return none;
        }
        let mut class = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
            return none;
        }

        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut info).as_bool() {
            return none;
        }
        let m = info.rcMonitor;
        let rect = [m.left, m.top, m.right - m.left, m.bottom - m.top];

        let shell_says = SHQueryUserNotificationState()
            .map(|s| s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE || s == QUNS_BUSY)
            .unwrap_or(false);

        let mut r = RECT::default();
        let covers = GetWindowRect(hwnd, &mut r).is_ok()
            && r.left <= m.left
            && r.top <= m.top
            && r.right >= m.right
            && r.bottom >= m.bottom;

        if covers || shell_says {
            FullscreenState { active: true, monitor: Some(rect) }
        } else {
            none
        }
    }
}

#[cfg(not(windows))]
fn detect() -> FullscreenState {
    FullscreenState { active: false, monitor: None }
}
