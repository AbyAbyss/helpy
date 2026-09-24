//! Overlay windows: one transparent, click-through, always-on-top window per monitor.
//!
//! Coordinates: everything crossing the IPC boundary is in *physical* desktop pixels.
//! Each overlay knows its monitor's physical origin and scale factor, and converts
//! to its own CSS pixels: `css = (global - origin) / scale`. This keeps mixed-DPI
//! setups correct without any window needing to know about the others.

mod fullscreen;
mod tracker;

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, Window,
};
use ts_rs::TS;

use crate::state::{update_runtime, AppState};

pub const LABEL_PREFIX: &str = "overlay-";
pub const GEOMETRY_EVENT: &str = "overlay://geometry";
pub const CLEAR_EVENT: &str = "overlay://clear";
pub const CHECK_EVENT: &str = "overlay://check";
pub const NOTICE_EVENT: &str = "overlay://notice";

#[derive(Serialize, Clone, Debug, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OverlayGeometry {
    pub label: String,
    pub index: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub primary: bool,
}

pub fn is_overlay(label: &str) -> bool {
    label.starts_with(LABEL_PREFIX)
}

fn current_monitors(app: &AppHandle) -> Vec<OverlayGeometry> {
    let primary = app.primary_monitor().ok().flatten().map(|m| *m.position());
    app.available_monitors()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(i, m)| OverlayGeometry {
            label: format!("{LABEL_PREFIX}{i}"),
            index: i as u32,
            name: m.name().cloned().unwrap_or_else(|| format!("Display {}", i + 1)),
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
            scale: m.scale_factor(),
            primary: primary == Some(*m.position()),
        })
        .collect()
}

fn build(app: &AppHandle, g: &OverlayGeometry) -> tauri::Result<WebviewWindow> {
    let w = WebviewWindowBuilder::new(app, &g.label, WebviewUrl::App("index.html".into()))
        .title("Helpy overlay")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        // Keeps the overlay out of screenshots and screen shares, including Helpy's own.
        .content_protected(true)
        .visible(false)
        .build()?;
    place(&w, g)?;
    // On Linux the GTK window has no native surface until it is shown, and setting the
    // input shape before that panics inside tao. Elsewhere, set it before showing so the
    // overlay never catches a click even for a frame.
    #[cfg(not(target_os = "linux"))]
    w.set_ignore_cursor_events(true)?;
    w.show()?;
    // Some platforms reset click-through on show, so set it (again) after.
    w.set_ignore_cursor_events(true)?;
    Ok(w)
}

fn place(w: &WebviewWindow, g: &OverlayGeometry) -> tauri::Result<()> {
    w.set_position(PhysicalPosition::new(g.x, g.y))?;
    w.set_size(PhysicalSize::new(g.width, g.height))?;
    Ok(())
}

/// Creates, moves, or closes overlays so there is exactly one per monitor.
pub fn sync(app: &AppHandle) {
    let wanted = current_monitors(app);
    let state = app.state::<AppState>();
    let changed = {
        let mut cur = state.overlays.lock().expect("overlays lock");
        if *cur == wanted {
            false
        } else {
            *cur = wanted.clone();
            true
        }
    };
    if !changed {
        return;
    }
    log::info!("displays changed, {} overlay(s)", wanted.len());

    for g in &wanted {
        match app.get_webview_window(&g.label) {
            Some(w) => {
                let _ = place(&w, g);
            }
            None => {
                if let Err(e) = build(app, g) {
                    log::error!("could not create overlay {}: {e}", g.label);
                }
            }
        }
    }
    for (label, w) in app.webview_windows() {
        if is_overlay(&label) && !wanted.iter().any(|g| g.label == label) {
            let _ = w.close();
        }
    }
    let _ = app.emit(GEOMETRY_EVENT, &wanted);
}

pub fn start(app: &AppHandle) {
    sync(app);
    tracker::spawn(app.clone());
    fullscreen::spawn(app.clone());
}

/// Shows or hides annotation state and owns the Escape shortcut, which is only
/// registered while something is on screen so Helpy never swallows Escape otherwise.
pub fn set_annotations_visible(app: &AppHandle, visible: bool) {
    update_runtime(app, |rt| rt.annotations_visible = visible);
    crate::hotkeys::set_escape_active(app, visible);
}

pub fn clear(app: &AppHandle) {
    let _ = app.emit(CLEAR_EVENT, ());
    set_annotations_visible(app, false);
}

/// Sends a short message to show next to the buddy (for example "voice arrives in a later build").
pub fn notice(app: &AppHandle, text: &str) {
    let _ = app.emit(NOTICE_EVENT, text);
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub fn overlay_geometry(window: Window, state: tauri::State<AppState>) -> Option<OverlayGeometry> {
    state
        .overlays
        .lock()
        .expect("overlays lock")
        .iter()
        .find(|g| g.label == window.label())
        .cloned()
}

#[tauri::command]
pub fn list_displays(state: tauri::State<AppState>) -> Vec<OverlayGeometry> {
    state.overlays.lock().expect("overlays lock").clone()
}

/// Draws a labeled frame on every overlay to confirm alignment and DPI handling.
#[tauri::command]
pub fn show_overlay_check(app: AppHandle) {
    let _ = app.emit(CHECK_EVENT, ());
    set_annotations_visible(&app, true);
}

#[tauri::command]
pub fn clear_annotations(app: AppHandle) {
    clear(&app);
}
