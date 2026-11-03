//! Apps from the Applications folders, opened with `open`; windows through
//! the Accessibility API, which needs Helpy allowed under Privacy &
//! Security → Accessibility (the same permission guidance uses).

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use accessibility_sys::{
    kAXErrorSuccess, kAXFrontmostAttribute, kAXHiddenAttribute, kAXMainAttribute, kAXMinimizedAttribute,
    kAXPositionAttribute, kAXRaiseAction, kAXSizeAttribute, kAXSubroleAttribute, kAXTitleAttribute,
    kAXValueTypeCGPoint, kAXValueTypeCGSize, kAXWindowsAttribute, AXIsProcessTrusted, AXUIElementCopyAttributeValue,
    AXUIElementCreateApplication, AXUIElementPerformAction, AXUIElementRef, AXUIElementSetAttributeValue,
    AXUIElementSetMessagingTimeout, AXValueCreate, AXValueGetValue, AXValueRef,
};
use core_foundation::array::CFArray;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::geometry::{CGPoint, CGSize};
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowLayer, kCGWindowListExcludeDesktopElements, kCGWindowOwnerName,
    kCGWindowOwnerPID,
};

use super::{App, Op, Rect, Win};

/// Windows move in points.
pub const POINTS: bool = true;

const FULL_SCREEN: &str = "AXFullScreen";
const NOT_TRUSTED: &str = "Helpy isn't allowed to control windows yet. Turn Helpy on in System Settings → Privacy & \
                           Security → Accessibility, then try again.";

pub fn apps() -> Vec<App> {
    let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
    let roots = [
        PathBuf::from("/Applications"),
        PathBuf::from("/Applications/Utilities"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        home.join("Applications"),
        home.join("Applications/Chrome Apps.localized"),
    ];
    let mut out = vec![App { name: "Finder".into(), path: "/System/Library/CoreServices/Finder.app".into() }];
    for root in roots {
        collect(&root, 0, &mut out);
    }
    out
}

/// .app bundles in a folder and its subfolders (vendor folders like
/// "Adobe Photoshop 2026"), without looking inside bundles.
fn collect(dir: &Path, depth: u32, out: &mut Vec<App>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().is_some_and(|x| x == "app") {
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                out.push(App { name: name.to_string(), path: path.clone() });
            }
        } else if depth < 1 && path.is_dir() {
            collect(&path, depth + 1, out);
        }
    }
}

pub fn launch(app: &App) -> Result<(), String> {
    let out = Command::new("open")
        .arg("-a")
        .arg(&app.path)
        .output()
        .map_err(|e| format!("Couldn't open {}: {e}", app.name))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("Couldn't open {}: {}", app.name, String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// A retained AX element.
struct Ax(CFType);

impl Ax {
    fn app(pid: i32) -> Option<Ax> {
        let r = unsafe { AXUIElementCreateApplication(pid) };
        if r.is_null() {
            return None;
        }
        let a = Ax(unsafe { CFType::wrap_under_create_rule(r as CFTypeRef) });
        unsafe { AXUIElementSetMessagingTimeout(a.raw(), 1.0) };
        Some(a)
    }

    fn raw(&self) -> AXUIElementRef {
        self.0.as_CFTypeRef() as AXUIElementRef
    }

    fn attr(&self, name: &str) -> Option<CFType> {
        let mut value: CFTypeRef = std::ptr::null();
        let key = CFString::new(name);
        let err = unsafe { AXUIElementCopyAttributeValue(self.raw(), key.as_concrete_TypeRef() as _, &mut value) };
        (err == kAXErrorSuccess && !value.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(value) })
    }

    fn string(&self, name: &str) -> Option<String> {
        self.attr(name)?.downcast::<CFString>().map(|s| s.to_string())
    }

    fn flag(&self, name: &str) -> bool {
        self.attr(name).and_then(|v| v.downcast::<CFBoolean>()).map(bool::from).unwrap_or(false)
    }

    fn set(&self, name: &str, value: &CFType) -> bool {
        let key = CFString::new(name);
        let err =
            unsafe { AXUIElementSetAttributeValue(self.raw(), key.as_concrete_TypeRef() as _, value.as_CFTypeRef()) };
        err == kAXErrorSuccess
    }

    fn set_flag(&self, name: &str, on: bool) -> bool {
        let v = if on { CFBoolean::true_value() } else { CFBoolean::false_value() };
        self.set(name, &v.as_CFType())
    }

    fn windows(&self) -> Vec<Ax> {
        let Some(list) = self.attr(kAXWindowsAttribute).and_then(|l| l.downcast::<CFArray>()) else {
            return Vec::new();
        };
        list.iter().map(|item| Ax(unsafe { CFType::wrap_under_get_rule(*item as CFTypeRef) })).collect()
    }

    fn rect(&self) -> Option<Rect> {
        let mut p = CGPoint::new(0.0, 0.0);
        let mut s = CGSize::new(0.0, 0.0);
        let pos = self.attr(kAXPositionAttribute)?;
        let size = self.attr(kAXSizeAttribute)?;
        let ok = unsafe {
            AXValueGetValue(pos.as_CFTypeRef() as AXValueRef, kAXValueTypeCGPoint, &mut p as *mut _ as *mut c_void)
                && AXValueGetValue(size.as_CFTypeRef() as AXValueRef, kAXValueTypeCGSize, &mut s as *mut _ as *mut c_void)
        };
        ok.then_some(Rect { x: p.x, y: p.y, w: s.width, h: s.height })
    }

    fn set_rect(&self, r: Rect) -> bool {
        let p = CGPoint::new(r.x, r.y);
        let s = CGSize::new(r.w, r.h);
        let pos = unsafe { AXValueCreate(kAXValueTypeCGPoint, &p as *const _ as *const c_void) };
        let size = unsafe { AXValueCreate(kAXValueTypeCGSize, &s as *const _ as *const c_void) };
        if pos.is_null() || size.is_null() {
            return false;
        }
        let pos = unsafe { CFType::wrap_under_create_rule(pos as CFTypeRef) };
        let size = unsafe { CFType::wrap_under_create_rule(size as CFTypeRef) };
        // Size first so it fits where it's going, then the place, then the
        // size again: apps clamp a size that doesn't fit their old screen.
        self.set(kAXSizeAttribute, &size);
        let moved = self.set(kAXPositionAttribute, &pos);
        let sized = self.set(kAXSizeAttribute, &size);
        moved || sized
    }

    fn standard(&self) -> bool {
        matches!(self.string(kAXSubroleAttribute).as_deref(), Some("AXStandardWindow") | Some("AXDialog") | None)
    }
}

/// Apps with ordinary windows, front to back, as (pid, name).
fn running() -> Vec<(i32, String)> {
    let Some(list) = copy_window_info(kCGWindowListExcludeDesktopElements, kCGNullWindowID) else {
        return Vec::new();
    };
    let own = std::process::id() as i64;
    let key = |k| unsafe { CFString::wrap_under_get_rule(k) };
    let mut out: Vec<(i32, String)> = Vec::new();
    for item in list.iter() {
        let dict: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
        let number = |k| dict.find(key(k)).and_then(|v| v.downcast::<CFNumber>()).and_then(|n| n.to_i64());
        let Some(pid) = number(unsafe { kCGWindowOwnerPID }) else { continue };
        if number(unsafe { kCGWindowLayer }) != Some(0) || pid == own || out.iter().any(|(p, _)| *p as i64 == pid) {
            continue;
        }
        let name = dict
            .find(key(unsafe { kCGWindowOwnerName }))
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
            .unwrap_or_default();
        out.push((pid as i32, name));
    }
    out
}

fn trusted() -> Result<(), String> {
    if unsafe { AXIsProcessTrusted() } {
        Ok(())
    } else {
        crate::a11y::request_trust();
        Err(NOT_TRUSTED.into())
    }
}

pub fn windows() -> Result<Vec<Win>, String> {
    trusted()?;
    let mut out = Vec::new();
    for (pid, name) in running() {
        let Some(app) = Ax::app(pid) else { continue };
        for (i, w) in app.windows().into_iter().enumerate() {
            if !w.standard() {
                continue;
            }
            let Some(frame) = w.rect() else { continue };
            out.push(Win {
                key: format!("{pid}:{i}"),
                pid: pid as u32,
                app: name.clone(),
                title: w.string(kAXTitleAttribute).unwrap_or_default(),
                frame,
                minimized: w.flag(kAXMinimizedAttribute),
                fullscreen: w.flag(FULL_SCREEN),
            });
        }
    }
    Ok(out)
}

/// The window again, by its place in its app's list, checked by title.
fn resolve(w: &Win) -> Result<(Ax, Ax), String> {
    let app = Ax::app(w.pid as i32).ok_or("That app isn't running any more.")?;
    let index: usize = w.key.split(':').nth(1).and_then(|i| i.parse().ok()).unwrap_or(0);
    let list = app.windows();
    let titled = |x: &Ax| x.string(kAXTitleAttribute).unwrap_or_default() == w.title;
    let win = if list.get(index).is_some_and(titled) {
        index
    } else {
        list.iter().position(titled).unwrap_or(index)
    };
    let win = list.into_iter().nth(win).ok_or("That window closed.")?;
    Ok((app, win))
}

/// Leaves full screen and un-minimizes, waiting for the animation, so the
/// window can be moved.
fn settle(win: &Ax) {
    if win.flag(FULL_SCREEN) {
        win.set_flag(FULL_SCREEN, false);
        std::thread::sleep(Duration::from_millis(900));
    }
    if win.flag(kAXMinimizedAttribute) {
        win.set_flag(kAXMinimizedAttribute, false);
        std::thread::sleep(Duration::from_millis(400));
    }
}

fn raise(app: &Ax, win: &Ax) {
    app.set_flag(kAXHiddenAttribute, false);
    app.set_flag(kAXFrontmostAttribute, true);
    win.set_flag(kAXMainAttribute, true);
    let action = CFString::new(kAXRaiseAction);
    unsafe { AXUIElementPerformAction(win.raw(), action.as_concrete_TypeRef() as _) };
}

pub fn act(w: &Win, op: &Op) -> Result<(), String> {
    trusted()?;
    if let Op::Quit = op {
        return quit(w.pid as i32, &w.app);
    }
    let (app, win) = resolve(w)?;
    let ok = match op {
        Op::Frame(r) | Op::Maximize(r) => {
            settle(&win);
            let ok = win.set_rect(*r);
            raise(&app, &win);
            ok
        }
        Op::FullScreen(on) => {
            if *on {
                settle(&win);
                raise(&app, &win);
            }
            win.set_flag(FULL_SCREEN, *on) || win.flag(FULL_SCREEN) == *on
        }
        Op::Minimize => win.set_flag(kAXMinimizedAttribute, true),
        Op::Restore => {
            app.set_flag(kAXHiddenAttribute, false);
            settle(&win);
            raise(&app, &win);
            true
        }
        Op::Focus => {
            settle(&win);
            raise(&app, &win);
            true
        }
        Op::Hide => app.set_flag(kAXHiddenAttribute, true),
        Op::Quit => unreachable!(),
    };
    if ok {
        Ok(())
    } else {
        Err(format!("{} didn't let Helpy change that window (some apps fix their window size).", w.app))
    }
}

/// Quits like ⌘Q, so the app can ask to save.
fn quit(pid: i32, name: &str) -> Result<(), String> {
    use objc2_app_kit::NSRunningApplication;
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .ok_or_else(|| format!("{name} isn't running any more."))?;
    if app.terminate() {
        Ok(())
    } else {
        Err(format!("{name} didn't agree to quit."))
    }
}
