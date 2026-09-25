//! UI Automation. Helpy is per-monitor DPI aware, so UIA's screen
//! coordinates are the same physical pixels as the rest of Helpy. Each
//! query runs on a blocking thread, where UIA sets up COM for itself.

use uiautomation::patterns::UIValuePattern;
use uiautomation::types::{ControlType, Handle, Point, TreeScope, UIProperty};
use uiautomation::variants::Variant;
use uiautomation::{UIAutomation, UIElement};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindow, GetWindowThreadProcessId, IsWindowVisible,
    GW_HWNDNEXT,
};

use super::{Element, Rect};

fn rect(e: &UIElement) -> Option<Rect> {
    let r = e.get_bounding_rectangle().ok()?;
    Some(Rect {
        x: r.get_left() as f64,
        y: r.get_top() as f64,
        w: (r.get_right() - r.get_left()) as f64,
        h: (r.get_bottom() - r.get_top()) as f64,
    })
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()
}

pub async fn element_at(x: f64, y: f64, _scale: f64) -> Option<Element> {
    blocking(move || {
        let a = UIAutomation::new().ok()?;
        let e = a
            .element_from_point(Point::new(x.round() as i32, y.round() as i32))
            .ok()?;
        Some(Element {
            rect: rect(&e)?,
            role: e.get_localized_control_type().unwrap_or_default(),
            name: e.get_name().unwrap_or_default(),
        })
    })
    .await
    .flatten()
}

fn window_element(a: &UIAutomation, hwnd: HWND) -> Option<UIElement> {
    a.element_from_handle(Handle::from(hwnd.0 as isize)).ok()
}

pub async fn password_fields(_scale: f64) -> Vec<Rect> {
    blocking(|| {
        let a = UIAutomation::new().ok()?;
        let w = window_element(&a, unsafe { GetForegroundWindow() })?;
        let cond = a
            .create_property_condition(UIProperty::IsPassword, Variant::from(true), None)
            .ok()?;
        let found = w.find_all(TreeScope::Descendants, &cond).ok()?;
        Some(found.iter().filter_map(rect).collect::<Vec<_>>())
    })
    .await
    .flatten()
    .unwrap_or_default()
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Chrome, Edge, Brave, Opera and Vivaldi share Chromium's window class.
fn is_browser(hwnd: HWND) -> bool {
    matches!(
        class_of(hwnd).as_str(),
        "Chrome_WidgetWin_1" | "MozillaWindowClass"
    )
}

/// The browser window in front, or the one under Helpy's own window when
/// that's in front (the user is typing the request).
fn front_browser() -> Option<HWND> {
    let own = std::process::id();
    let mut hwnd = unsafe { GetForegroundWindow() };
    for _ in 0..200 {
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid != own && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return is_browser(hwnd).then_some(hwnd);
        }
        hwnd = unsafe { GetWindow(hwnd, GW_HWNDNEXT) }.ok()?;
    }
    None
}

pub async fn browser_url() -> Option<String> {
    blocking(|| {
        let hwnd = front_browser()?;
        let a = UIAutomation::new().ok()?;
        let w = window_element(&a, hwnd)?;
        let cond = a
            .create_property_condition(
                UIProperty::ControlType,
                Variant::from(ControlType::Edit as i32),
                None,
            )
            .ok()?;
        for e in w.find_all(TreeScope::Descendants, &cond).ok()? {
            let name = e.get_name().unwrap_or_default().to_lowercase();
            let id = e.get_automation_id().unwrap_or_default();
            if !(name.contains("address") || id == "urlbar-input") {
                continue;
            }
            if let Ok(v) = e
                .get_pattern::<UIValuePattern>()
                .and_then(|p| p.get_value())
            {
                if !v.trim().is_empty() {
                    return Some(v);
                }
            }
        }
        None
    })
    .await
    .flatten()
}
