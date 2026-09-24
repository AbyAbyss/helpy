//! Polls the global cursor and broadcasts moves to the overlays.
//!
//! Polling at ~120 Hz and emitting only on change keeps IPC traffic near zero while the
//! mouse is still and fast enough that the buddy never visibly trails the pointer.
//! The same loop re-checks the display layout every two seconds.

use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget};
use ts_rs::TS;

use super::is_overlay;

pub const CURSOR_EVENT: &str = "cursor://move";

#[derive(Serialize, Clone, Copy, Debug, TS)]
#[ts(export)]
pub struct CursorPoint {
    /// Physical desktop pixels.
    pub x: f64,
    pub y: f64,
}

const POLL: Duration = Duration::from_millis(8);
const DISPLAY_CHECK: Duration = Duration::from_secs(2);

pub fn spawn(app: AppHandle) {
    thread::Builder::new()
        .name("helpy-cursor".into())
        .spawn(move || {
            let mut last = (f64::NAN, f64::NAN);
            let mut last_display_check = Instant::now();
            loop {
                if let Ok(p) = app.cursor_position() {
                    if (p.x, p.y) != last {
                        last = (p.x, p.y);
                        let _ = app.emit_filter(CURSOR_EVENT, CursorPoint { x: p.x, y: p.y }, |t| {
                            matches!(t, EventTarget::WebviewWindow { label } if is_overlay(label))
                        });
                    }
                }
                if last_display_check.elapsed() >= DISPLAY_CHECK {
                    last_display_check = Instant::now();
                    super::sync(&app);
                }
                thread::sleep(POLL);
            }
        })
        .expect("spawn cursor thread");
}
