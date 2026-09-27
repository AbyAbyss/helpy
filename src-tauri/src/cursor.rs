//! Tracks the mouse and tells each overlay where the cursor is, in that
//! overlay's own logical pixels. Also decides whether the buddy is visible
//! (enabled, not in a fullscreen app, not idle).

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use crate::fullscreen;
use crate::overlay::{self, Overlays};
use crate::settings::SettingsStore;

pub const CURSOR_EVENT: &str = "overlay://cursor";
pub const VISIBLE_EVENT: &str = "buddy://visible";

const POLL: Duration = Duration::from_millis(8);
const FULLSCREEN_CHECK: Duration = Duration::from_millis(500);
const MONITOR_CHECK: Duration = Duration::from_secs(2);

/// Cursor position for one overlay. `inside` is false when the cursor is on
/// another monitor.
#[derive(Serialize, TS, Clone, Copy, Debug)]
#[ts(export)]
pub struct CursorFrame {
    pub x: f64,
    pub y: f64,
    pub inside: bool,
}

#[derive(Default)]
pub struct CursorShared {
    visible: AtomicBool,
    /// Set when an overlay (re)loads so the next poll re-sends everything.
    resend: AtomicBool,
}

pub fn spawn(app: AppHandle) {
    thread::Builder::new()
        .name("helpy-cursor".into())
        .spawn(move || run(app))
        .expect("spawn cursor thread");
}

fn run(app: AppHandle) {
    let shared = app.state::<CursorShared>();
    let mut detector = fullscreen::Detector::new();
    let mut last_pos: Option<(f64, f64)> = None;
    let mut last_label: Option<String> = None;
    let mut last_move = Instant::now();
    let mut last_fs_check = Instant::now() - FULLSCREEN_CHECK;
    let mut last_monitor_check = Instant::now();
    let mut fullscreen = false;
    let mut last_visible: Option<bool> = None;

    loop {
        thread::sleep(POLL);
        let buddy = app.state::<SettingsStore>().get().buddy;
        let resend = shared.resend.swap(false, Ordering::Relaxed);

        if last_monitor_check.elapsed() >= MONITOR_CHECK {
            last_monitor_check = Instant::now();
            overlay::sync(&app);
        }
        if buddy.hide_in_fullscreen && last_fs_check.elapsed() >= FULLSCREEN_CHECK {
            last_fs_check = Instant::now();
            fullscreen = detector.is_fullscreen_active();
        }

        // Wayland without XWayland has no global cursor position.
        let Ok(pos) = app.cursor_position() else {
            continue;
        };
        let pos = (pos.x, pos.y);
        let moved = last_pos != Some(pos);
        if moved {
            last_move = Instant::now();
        }

        let idle = buddy.hide_when_idle
            && last_move.elapsed() >= Duration::from_secs(buddy.idle_seconds as u64);
        let visible = buddy.enabled && !(buddy.hide_in_fullscreen && fullscreen) && !idle;
        if last_visible != Some(visible) || resend {
            last_visible = Some(visible);
            shared.visible.store(visible, Ordering::Relaxed);
            let _ = app.emit(VISIBLE_EVENT, visible);
        }

        if !moved && !resend {
            continue;
        }
        last_pos = Some(pos);

        let overlays = app.state::<Overlays>().snapshot();
        let Some((label, monitor)) = overlays.iter().find(|(_, m)| m.contains(pos.0, pos.1)) else {
            continue;
        };
        if last_label.as_deref() != Some(label.as_str()) || resend {
            for (other, _) in overlays.iter().filter(|(l, _)| l != label) {
                let _ = app.emit_to(
                    other.as_str(),
                    CURSOR_EVENT,
                    CursorFrame {
                        x: 0.0,
                        y: 0.0,
                        inside: false,
                    },
                );
            }
            last_label = Some(label.clone());
        }
        let (x, y) = monitor.to_local_logical(pos.0, pos.1);
        let _ = app.emit_to(
            label.as_str(),
            CURSOR_EVENT,
            CursorFrame { x, y, inside: true },
        );
    }
}

/// Whether the buddy is showing next to the cursor right now.
pub fn buddy_shown(app: &AppHandle) -> bool {
    app.state::<CursorShared>().visible.load(Ordering::Relaxed)
}

/// Called by an overlay when it loads. Returns whether the buddy should be
/// visible and schedules a fresh cursor frame.
#[tauri::command]
pub fn overlay_ready(shared: tauri::State<CursorShared>) -> bool {
    shared.resend.store(true, Ordering::Relaxed);
    shared.visible.load(Ordering::Relaxed)
}
