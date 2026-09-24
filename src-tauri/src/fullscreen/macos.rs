//! macOS: the frontmost normal-layer window of another app covers a whole
//! display, including the menu bar area. Maximized windows don't (the menu bar
//! stays), fullscreen Spaces and presentations do.

use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::display::{CGDisplay, CGRect};
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer,
    kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, kCGWindowOwnerPID,
};

pub struct Detector;

impl Detector {
    pub fn new() -> Self {
        Self
    }

    pub fn is_fullscreen_active(&mut self) -> bool {
        front_window_bounds().is_some_and(|bounds| {
            CGDisplay::active_displays()
                .unwrap_or_default()
                .into_iter()
                .map(|id| CGDisplay::new(id).bounds())
                .any(|d| same_rect(&d, &bounds))
        })
    }
}

fn front_window_bounds() -> Option<CGRect> {
    let windows = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    )?;
    let own_pid = std::process::id() as i64;
    let key = |k| unsafe { CFString::wrap_under_get_rule(k) };
    for item in windows.iter() {
        let dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
        let number = |k| {
            dict.find(key(k))
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_i64())
        };
        // Layer 0 is where normal app windows live; menus, docks and our own
        // overlays sit on other layers.
        if number(unsafe { kCGWindowLayer }) != Some(0)
            || number(unsafe { kCGWindowOwnerPID }) == Some(own_pid)
        {
            continue;
        }
        let bounds = dict
            .find(key(unsafe { kCGWindowBounds }))?
            .downcast::<CFDictionary>()?;
        return CGRect::from_dict_representation(&bounds);
    }
    None
}

fn same_rect(a: &CGRect, b: &CGRect) -> bool {
    (a.origin.x - b.origin.x).abs() < 1.0
        && (a.origin.y - b.origin.y).abs() < 1.0
        && (a.size.width - b.size.width).abs() < 1.0
        && (a.size.height - b.size.height).abs() < 1.0
}
