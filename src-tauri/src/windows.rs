//! Helpy's own app windows (not the overlays).

use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition};

use crate::overlay::Overlays;

pub const SETTINGS: &str = "settings";
pub const ASK: &str = "ask";
pub const PILL: &str = "pill";

/// Voice pill size in logical pixels (matches tauri.conf.json).
const PILL_SIZE: (f64, f64) = (300.0, 120.0);
pub const OPEN_SECTION_EVENT: &str = "settings://open-section";

/// Ask panel size in logical pixels (matches tauri.conf.json).
const ASK_SIZE: (f64, f64) = (440.0, 560.0);
/// Gap between the cursor and the panel, logical pixels.
const GAP: f64 = 18.0;

pub fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Where the panel's top-left corner goes: beside the cursor, flipped to the
/// other side when it wouldn't fit, and always inside the monitor.
pub fn place_near(
    cursor: (f64, f64),
    monitor: (f64, f64, f64, f64),
    size: (f64, f64),
    gap: f64,
) -> (f64, f64) {
    let (mx, my, mw, mh) = monitor;
    let (w, h) = size;
    let mut x = cursor.0 + gap;
    if x + w > mx + mw {
        x = cursor.0 - gap - w;
    }
    let mut y = cursor.1 + gap;
    if y + h > my + mh {
        y = cursor.1 - gap - h;
    }
    (
        x.clamp(mx, (mx + mw - w).max(mx)),
        y.clamp(my, (my + mh - h).max(my)),
    )
}

/// Opens the ask panel next to the cursor and focuses it.
pub fn show_ask(app: &AppHandle) {
    let Some(w) = app.get_webview_window(ASK) else {
        return;
    };
    if let Ok(cursor) = app.cursor_position() {
        let monitors = app.state::<Overlays>().snapshot();
        if let Some((_, m)) = monitors
            .iter()
            .find(|(_, m)| m.contains(cursor.x, cursor.y))
        {
            let size = (ASK_SIZE.0 * m.scale, ASK_SIZE.1 * m.scale);
            let (x, y) = place_near(
                (cursor.x, cursor.y),
                (m.x as f64, m.y as f64, m.width as f64, m.height as f64),
                size,
                GAP * m.scale,
            );
            let _ = w.set_size(LogicalSize::new(ASK_SIZE.0, ASK_SIZE.1));
            let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        }
    }
    let _ = w.show();
    let _ = w.set_always_on_top(true);
    let _ = w.set_focus();
    let _ = w.emit_to(ASK, "ask://shown", ());
}

/// Shows the voice waveform just right of the cursor, without taking focus
/// from the app the user is in.
pub fn show_pill(app: &AppHandle) {
    let Some(w) = app.get_webview_window(PILL) else {
        return;
    };
    if let Ok(cursor) = app.cursor_position() {
        let monitors = app.state::<Overlays>().snapshot();
        if let Some((_, m)) = monitors
            .iter()
            .find(|(_, m)| m.contains(cursor.x, cursor.y))
        {
            let size = (PILL_SIZE.0 * m.scale, PILL_SIZE.1 * m.scale);
            // Sit level with the cursor, where the eye already is.
            let at = (cursor.x, cursor.y - 22.0 * m.scale);
            let (x, y) = place_near(
                at,
                (m.x as f64, m.y as f64, m.width as f64, m.height as f64),
                size,
                14.0 * m.scale,
            );
            let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        }
    }
    let _ = w.show();
    let _ = w.set_always_on_top(true);
}

pub fn hide_pill(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PILL) {
        let _ = w.hide();
    }
}

#[tauri::command]
pub fn ask_hide(app: AppHandle) {
    if let Some(w) = app.get_webview_window(ASK) {
        let _ = w.hide();
    }
}

/// Opens settings at a section, e.g. from an error in the ask panel.
#[tauri::command]
pub fn open_settings_section(app: AppHandle, section: String) {
    show_settings(&app);
    let _ = app.emit_to(SETTINGS, OPEN_SECTION_EVENT, section);
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: (f64, f64, f64, f64) = (0.0, 0.0, 1920.0, 1080.0);

    #[test]
    fn opens_below_right_of_the_cursor() {
        assert_eq!(
            place_near((100.0, 100.0), MON, (440.0, 560.0), 18.0),
            (118.0, 118.0)
        );
    }

    #[test]
    fn flips_left_and_up_near_the_bottom_right_corner() {
        assert_eq!(
            place_near((1900.0, 1000.0), MON, (440.0, 560.0), 18.0),
            (1442.0, 422.0)
        );
    }

    #[test]
    fn stays_on_a_secondary_monitor_with_negative_coordinates() {
        let m = (-2560.0, 0.0, 2560.0, 1440.0);
        let (x, y) = place_near((-10.0, 5.0), m, (880.0, 1120.0), 36.0);
        assert!(x >= -2560.0 && x + 880.0 <= 0.0);
        assert!(y >= 0.0 && y + 1120.0 <= 1440.0);
    }
}
