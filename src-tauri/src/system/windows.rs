//! Apps from the Start menu (Get-StartApps, so Store apps are found too,
//! else the Start menu's shortcuts); windows through Win32.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongW, GetWindowRect, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed, PostMessageW, SetForegroundWindow,
    SetWindowPos, ShowWindow, GWL_EXSTYLE, GW_OWNER, SWP_NOACTIVATE, SWP_NOZORDER, SW_MAXIMIZE, SW_MINIMIZE,
    SW_RESTORE, WM_CLOSE, WS_EX_TOOLWINDOW,
};

use super::{App, Op, Rect, Win};

/// Windows move in pixels.
pub const POINTS: bool = false;

const NO_WINDOW: u32 = 0x0800_0000;

pub fn apps() -> Vec<App> {
    let mut out = start_apps();
    if out.is_empty() {
        for root in [std::env::var("ProgramData"), std::env::var("APPDATA")].into_iter().flatten() {
            shortcuts(&Path::new(&root).join(r"Microsoft\Windows\Start Menu\Programs"), &mut out);
        }
    }
    out
}

/// Every app in the Start menu, with the id that opens it.
fn start_apps() -> Vec<App> {
    let Ok(out) = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", "Get-StartApps | ForEach-Object { $_.Name + \"`t\" + $_.AppID }"])
        .creation_flags(NO_WINDOW)
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(n, id)| !n.trim().is_empty() && !id.trim().is_empty())
        .map(|(n, id)| App { name: n.trim().to_string(), path: PathBuf::from(format!("shell:AppsFolder\\{}", id.trim())) })
        .collect()
}

fn shortcuts(dir: &Path, out: &mut Vec<App>) {
    for e in walkdir::WalkDir::new(dir).max_depth(3).into_iter().flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
            if let Some(name) = p.file_stem().and_then(|s| s.to_str()) {
                if !name.to_lowercase().contains("uninstall") {
                    out.push(App { name: name.to_string(), path: p.to_path_buf() });
                }
            }
        }
    }
}

pub fn launch(app: &App) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(&app.path)
        .creation_flags(NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't open {}: {e}", app.name))
}

fn hwnd(key: &str) -> HWND {
    HWND(key.parse::<isize>().unwrap_or(0) as *mut _)
}

fn text(h: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(h) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn process_name(pid: u32) -> String {
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return String::new();
    };
    let mut buf = vec![0u16; 1024];
    let mut size = buf.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut size)
    };
    let _ = unsafe { CloseHandle(handle) };
    if ok.is_err() {
        return String::new();
    }
    let path = String::from_utf16_lossy(&buf[..size as usize]);
    Path::new(&path).file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string()
}

fn cloaked(h: HWND) -> bool {
    let mut v: u32 = 0;
    unsafe { DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut v as *mut _ as *mut _, 4) }.is_ok() && v != 0
}

fn rect_of(h: HWND) -> Rect {
    let mut r = RECT::default();
    let _ = unsafe { GetWindowRect(h, &mut r) };
    Rect { x: r.left as f64, y: r.top as f64, w: (r.right - r.left) as f64, h: (r.bottom - r.top) as f64 }
}

/// The invisible resize border around a window, which DWM leaves out of
/// what's drawn: left, top, right, bottom.
fn border(h: HWND) -> (i32, i32, i32, i32) {
    let mut outer = RECT::default();
    let mut shown = RECT::default();
    let ok = unsafe { GetWindowRect(h, &mut outer) }.is_ok()
        && unsafe {
            DwmGetWindowAttribute(
                h,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut shown as *mut _ as *mut _,
                std::mem::size_of::<RECT>() as u32,
            )
        }
        .is_ok();
    if !ok {
        return (0, 0, 0, 0);
    }
    (shown.left - outer.left, shown.top - outer.top, outer.right - shown.right, outer.bottom - shown.bottom)
}

unsafe extern "system" fn each(h: HWND, list: LPARAM) -> BOOL {
    let list = &mut *(list.0 as *mut Vec<HWND>);
    list.push(h);
    BOOL(1)
}

pub fn windows() -> Result<Vec<Win>, String> {
    let mut all: Vec<HWND> = Vec::new();
    unsafe { EnumWindows(Some(each), LPARAM(&mut all as *mut _ as isize)) }.map_err(|e| e.to_string())?;
    let own = std::process::id();
    let mut out = Vec::new();
    for h in all {
        let visible = unsafe { IsWindowVisible(h) }.as_bool();
        let owned = unsafe { GetWindow(h, GW_OWNER) }.is_ok_and(|o| !o.0.is_null());
        let tool = unsafe { GetWindowLongW(h, GWL_EXSTYLE) } as u32 & WS_EX_TOOLWINDOW.0 != 0;
        let title = text(h);
        if !visible || owned || tool || title.is_empty() || cloaked(h) {
            continue;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
        if pid == own {
            continue;
        }
        let minimized = unsafe { IsIconic(h) }.as_bool();
        out.push(Win {
            key: (h.0 as isize).to_string(),
            pid,
            app: process_name(pid),
            title,
            frame: rect_of(h),
            minimized,
            fullscreen: false,
        });
    }
    Ok(out)
}

/// Brings a window forward, past Windows' rule that only the app in front
/// may change what's in front.
fn focus(h: HWND) {
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let front = GetForegroundWindow();
        let theirs = GetWindowThreadProcessId(front, None);
        let ours = GetCurrentThreadId();
        let attached = theirs != ours && AttachThreadInput(ours, theirs, true).as_bool();
        let _ = BringWindowToTop(h);
        let _ = SetForegroundWindow(h);
        if attached {
            let _ = AttachThreadInput(ours, theirs, false);
        }
    }
}

pub fn act(w: &Win, op: &Op) -> Result<(), String> {
    let h = hwnd(&w.key);
    unsafe {
        match op {
            Op::Frame(r) => {
                if IsIconic(h).as_bool() || IsZoomed(h).as_bool() {
                    let _ = ShowWindow(h, SW_RESTORE);
                }
                let (l, t, rr, b) = border(h);
                SetWindowPos(
                    h,
                    None,
                    r.x as i32 - l,
                    r.y as i32 - t,
                    r.w as i32 + l + rr,
                    r.h as i32 + t + b,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
                .map_err(|e| format!("{} didn't let Helpy move that window: {e}", w.app))?;
                focus(h);
            }
            Op::Maximize(r) => {
                // Onto the right screen first, then the system's own maximize.
                if IsZoomed(h).as_bool() {
                    let _ = ShowWindow(h, SW_RESTORE);
                }
                let _ = SetWindowPos(h, None, r.x as i32, r.y as i32, r.w as i32, r.h as i32, SWP_NOZORDER);
                let _ = ShowWindow(h, SW_MAXIMIZE);
                focus(h);
            }
            Op::FullScreen(true) => {
                let _ = ShowWindow(h, SW_MAXIMIZE);
                focus(h);
            }
            Op::FullScreen(false) | Op::Restore => {
                let _ = ShowWindow(h, SW_RESTORE);
                focus(h);
            }
            // Hidden windows leave the taskbar, so hiding minimizes.
            Op::Minimize | Op::Hide => {
                let _ = ShowWindow(h, SW_MINIMIZE);
            }
            Op::Focus => focus(h),
            Op::Quit => {
                // Closes each of its windows, as their close buttons do.
                for other in windows()?.into_iter().filter(|x| x.pid == w.pid) {
                    let _ = PostMessageW(hwnd(&other.key), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }
    }
    Ok(())
}
