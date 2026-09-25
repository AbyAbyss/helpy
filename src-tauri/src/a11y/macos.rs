//! The AX API. It works in points, so positions are divided by the
//! monitor's scale going in and multiplied coming out. Nothing works until
//! the user allows Helpy under Privacy & Security → Accessibility; until
//! then every query just comes back empty.

use std::collections::VecDeque;
use std::ffi::c_void;

use accessibility_sys::{
    kAXChildrenAttribute, kAXDescriptionAttribute, kAXErrorSuccess, kAXFocusedApplicationAttribute,
    kAXFocusedWindowAttribute, kAXMainWindowAttribute, kAXPositionAttribute, kAXRoleAttribute,
    kAXSizeAttribute, kAXSubroleAttribute, kAXTitleAttribute, kAXURLAttribute, kAXValueAttribute,
    kAXValueTypeCGPoint, kAXValueTypeCGSize, AXIsProcessTrusted, AXUIElementCopyAttributeValue,
    AXUIElementCopyElementAtPosition, AXUIElementCreateApplication, AXUIElementCreateSystemWide,
    AXUIElementRef, AXUIElementSetMessagingTimeout, AXValueGetValue, AXValueRef,
};
use core_foundation::array::CFArray;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_foundation::url::CFURL;
use core_graphics::geometry::{CGPoint, CGSize};
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowLayer, kCGWindowListExcludeDesktopElements,
    kCGWindowListOptionOnScreenOnly, kCGWindowOwnerName, kCGWindowOwnerPID,
};

use super::{Element, Rect};

/// Most elements looked at in one search, so a huge page can't stall it.
const MAX_NODES: usize = 3000;

/// A retained AX element.
struct Ax(CFType);

impl Ax {
    fn from_create(r: AXUIElementRef) -> Option<Self> {
        (!r.is_null()).then(|| Ax(unsafe { CFType::wrap_under_create_rule(r as CFTypeRef) }))
    }

    fn raw(&self) -> AXUIElementRef {
        self.0.as_CFTypeRef() as AXUIElementRef
    }

    fn attr(&self, name: &str) -> Option<CFType> {
        let mut value: CFTypeRef = std::ptr::null();
        let key = CFString::new(name);
        let err = unsafe {
            AXUIElementCopyAttributeValue(self.raw(), key.as_concrete_TypeRef() as _, &mut value)
        };
        (err == kAXErrorSuccess && !value.is_null())
            .then(|| unsafe { CFType::wrap_under_create_rule(value) })
    }

    fn string(&self, name: &str) -> Option<String> {
        self.attr(name)?
            .downcast::<CFString>()
            .map(|s| s.to_string())
    }

    fn element(&self, name: &str) -> Option<Ax> {
        self.attr(name).map(Ax)
    }

    fn children(&self) -> Vec<Ax> {
        let Some(list) = self.attr(kAXChildrenAttribute) else {
            return Vec::new();
        };
        let Some(list) = list.downcast::<CFArray>() else {
            return Vec::new();
        };
        list.iter()
            .map(|item| Ax(unsafe { CFType::wrap_under_get_rule(*item as CFTypeRef) }))
            .collect()
    }

    /// Bounds in points.
    fn rect(&self) -> Option<(f64, f64, f64, f64)> {
        let mut p = CGPoint::new(0.0, 0.0);
        let mut s = CGSize::new(0.0, 0.0);
        let pos = self.attr(kAXPositionAttribute)?;
        let size = self.attr(kAXSizeAttribute)?;
        let ok = unsafe {
            AXValueGetValue(
                pos.as_CFTypeRef() as AXValueRef,
                kAXValueTypeCGPoint,
                &mut p as *mut _ as *mut c_void,
            ) && AXValueGetValue(
                size.as_CFTypeRef() as AXValueRef,
                kAXValueTypeCGSize,
                &mut s as *mut _ as *mut c_void,
            )
        };
        ok.then_some((p.x, p.y, s.width, s.height))
    }
}

fn physical((x, y, w, h): (f64, f64, f64, f64), scale: f64) -> Rect {
    Rect {
        x: x * scale,
        y: y * scale,
        w: w * scale,
        h: h * scale,
    }
}

fn system() -> Option<Ax> {
    if !unsafe { AXIsProcessTrusted() } {
        return None;
    }
    let sys = Ax::from_create(unsafe { AXUIElementCreateSystemWide() })?;
    unsafe { AXUIElementSetMessagingTimeout(sys.raw(), 0.5) };
    Some(sys)
}

/// Breadth-first search under `root` for the first element `f` accepts,
/// or all of them.
fn search<T>(root: Ax, mut f: impl FnMut(&Ax) -> Option<T>, all: bool) -> Vec<T> {
    let mut out = Vec::new();
    let mut queue = VecDeque::from([root]);
    let mut seen = 0;
    while let Some(e) = queue.pop_front() {
        seen += 1;
        if seen > MAX_NODES {
            break;
        }
        if let Some(t) = f(&e) {
            out.push(t);
            if !all {
                break;
            }
        }
        queue.extend(e.children());
    }
    out
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()
}

pub async fn element_at(x: f64, y: f64, scale: f64) -> Option<Element> {
    blocking(move || {
        let sys = system()?;
        let mut hit: AXUIElementRef = std::ptr::null_mut();
        let err = unsafe {
            AXUIElementCopyElementAtPosition(
                sys.raw(),
                (x / scale) as f32,
                (y / scale) as f32,
                &mut hit,
            )
        };
        if err != kAXErrorSuccess {
            return None;
        }
        let e = Ax::from_create(hit)?;
        Some(Element {
            rect: physical(e.rect()?, scale),
            role: e.string(kAXRoleAttribute).unwrap_or_default(),
            name: e
                .string(kAXTitleAttribute)
                .or_else(|| e.string(kAXDescriptionAttribute))
                .unwrap_or_default(),
        })
    })
    .await
    .flatten()
}

pub async fn password_fields(scale: f64) -> Vec<Rect> {
    blocking(move || {
        let app = system()?.element(kAXFocusedApplicationAttribute)?;
        let window = app.element(kAXFocusedWindowAttribute)?;
        Some(search(
            window,
            |e| {
                (e.string(kAXSubroleAttribute).as_deref() == Some("AXSecureTextField"))
                    .then(|| e.rect().map(|r| physical(r, scale)))
                    .flatten()
            },
            true,
        ))
    })
    .await
    .flatten()
    .unwrap_or_default()
}

const BROWSERS: [&str; 9] = [
    "Google Chrome",
    "Safari",
    "Firefox",
    "Microsoft Edge",
    "Brave Browser",
    "Arc",
    "Opera",
    "Vivaldi",
    "Chromium",
];

/// The process of the frontmost window that isn't Helpy's, if it's a
/// browser's (so it's found even while Helpy's panel is in front).
fn front_browser_pid() -> Option<i32> {
    let windows = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    )?;
    let own = std::process::id() as i64;
    let key = |k| unsafe { CFString::wrap_under_get_rule(k) };
    for item in windows.iter() {
        let dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
        let number = |k| {
            dict.find(key(k))
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_i64())
        };
        let pid = number(unsafe { kCGWindowOwnerPID });
        if number(unsafe { kCGWindowLayer }) != Some(0) || pid == Some(own) {
            continue;
        }
        let owner = dict
            .find(key(unsafe { kCGWindowOwnerName }))
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
            .unwrap_or_default();
        return BROWSERS
            .contains(&owner.as_str())
            .then(|| pid.map(|p| p as i32))
            .flatten();
    }
    None
}

pub async fn browser_url() -> Option<String> {
    blocking(|| {
        system()?;
        let pid = front_browser_pid()?;
        let app = Ax::from_create(unsafe { AXUIElementCreateApplication(pid) })?;
        unsafe { AXUIElementSetMessagingTimeout(app.raw(), 0.5) };
        let window = app
            .element(kAXFocusedWindowAttribute)
            .or_else(|| app.element(kAXMainWindowAttribute))?;
        search(
            window,
            |e| {
                let role = e.string(kAXRoleAttribute)?;
                if role == "AXWebArea" {
                    return e
                        .attr(kAXURLAttribute)?
                        .downcast::<CFURL>()
                        .map(|u| u.get_string().to_string());
                }
                let about = e
                    .string(kAXDescriptionAttribute)
                    .unwrap_or_default()
                    .to_lowercase();
                (role == "AXTextField" && about.contains("address"))
                    .then(|| e.string(kAXValueAttribute))
                    .flatten()
            },
            false,
        )
        .pop()
    })
    .await
    .flatten()
}
