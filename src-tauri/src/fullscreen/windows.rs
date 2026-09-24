//! Windows: the shell's own "user is busy" state (covers D3D fullscreen and
//! presentation mode), plus a check that the foreground window covers its
//! whole monitor (borderless fullscreen games and video players).

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
};

pub struct Detector;

impl Detector {
    pub fn new() -> Self {
        Self
    }

    pub fn is_fullscreen_active(&mut self) -> bool {
        unsafe {
            if let Ok(state) = SHQueryUserNotificationState() {
                if state == QUNS_BUSY
                    || state == QUNS_RUNNING_D3D_FULL_SCREEN
                    || state == QUNS_PRESENTATION_MODE
                {
                    return true;
                }
            }
            foreground_covers_monitor()
        }
    }
}

unsafe fn foreground_covers_monitor() -> bool {
    let hwnd = GetForegroundWindow();
    if hwnd.is_invalid() || is_desktop(hwnd) {
        return false;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == std::process::id() {
        return false;
    }
    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_err() {
        return false;
    }
    let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut info).as_bool() {
        return false;
    }
    let m = info.rcMonitor;
    rect.left <= m.left && rect.top <= m.top && rect.right >= m.right && rect.bottom >= m.bottom
}

/// The desktop wallpaper windows cover the monitor too.
unsafe fn is_desktop(hwnd: HWND) -> bool {
    let mut buf = [0u16; 32];
    let len = GetClassNameW(hwnd, &mut buf) as usize;
    let class = String::from_utf16_lossy(&buf[..len]);
    class == "Progman" || class == "WorkerW"
}
