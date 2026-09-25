//! One transparent, click-through, capture-excluded window per monitor.
//! Everything Helpy draws on top of other apps renders here.

use std::sync::Mutex;

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

pub const CLEAR_EVENT: &str = "overlay://clear";

/// A monitor's bounds in physical pixels plus its scale factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

impl MonitorRect {
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x as f64
            && py >= self.y as f64
            && px < self.x as f64 + self.width as f64
            && py < self.y as f64 + self.height as f64
    }

    /// Converts a global physical point to this monitor's logical (CSS) pixels.
    pub fn to_local_logical(self, px: f64, py: f64) -> (f64, f64) {
        (
            (px - self.x as f64) / self.scale,
            (py - self.y as f64) / self.scale,
        )
    }
}

#[derive(Default)]
pub struct Overlays {
    /// (window label, monitor) for every live overlay.
    windows: Mutex<Vec<(String, MonitorRect)>>,
    generation: Mutex<u32>,
}

impl Overlays {
    pub fn snapshot(&self) -> Vec<(String, MonitorRect)> {
        self.windows.lock().unwrap().clone()
    }
}

fn current_monitors(app: &AppHandle) -> Vec<MonitorRect> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| MonitorRect {
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
            scale: m.scale_factor(),
        })
        .collect()
}

/// Creates overlays to match the current monitor layout. Does nothing if the
/// layout is unchanged, so it is cheap to call on a timer.
pub fn sync(app: &AppHandle) {
    let monitors = current_monitors(app);
    let state = app.state::<Overlays>();
    let existing = state.snapshot();
    if existing
        .iter()
        .map(|(_, m)| *m)
        .eq(monitors.iter().copied())
    {
        return;
    }

    for (label, _) in &existing {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.destroy();
        }
    }

    // Labels must be unique even while old windows are still closing.
    let generation = {
        let mut g = state.generation.lock().unwrap();
        *g += 1;
        *g
    };
    let mut created = Vec::new();
    for (i, m) in monitors.iter().enumerate() {
        let label = format!("overlay-{generation}-{i}");
        match build(app, &label, m) {
            Ok(()) => created.push((label, *m)),
            Err(e) => log::error!("couldn't create overlay for monitor {i}: {e}"),
        }
    }
    *state.windows.lock().unwrap() = created;
}

fn build(app: &AppHandle, label: &str, m: &MonitorRect) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::App("overlay.html".into()))
        .title("Helpy overlay")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        .focusable(false)
        .content_protected(true)
        .visible(false)
        .build()?;
    // Position in physical pixels after creation: builder positions are
    // logical and would be scaled by the wrong monitor's DPI.
    window.set_position(PhysicalPosition::new(m.x, m.y))?;
    window.set_size(PhysicalSize::new(m.width, m.height))?;
    // Click-through must come after show(): on Linux the native window only
    // exists once shown, and tao panics otherwise. Both calls run in order on
    // the event loop, and the page is fully transparent until then.
    window.show()?;
    window.set_ignore_cursor_events(true)?;
    window.set_always_on_top(true)?;
    // Showing can move the window: macOS pushes it below the menu bar,
    // which shifted every mark down by the bar's height. Setting the frame
    // again once shown puts it back over the whole monitor.
    window.set_position(PhysicalPosition::new(m.x, m.y))?;
    window.set_size(PhysicalSize::new(m.width, m.height))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_to_local_logical_on_scaled_secondary_monitor() {
        // A 4K monitor at 200% to the right of a 1080p one.
        let m = MonitorRect {
            x: 1920,
            y: 0,
            width: 3840,
            height: 2160,
            scale: 2.0,
        };
        assert!(m.contains(1920.0, 0.0));
        assert!(!m.contains(1919.0, 10.0));
        assert!(!m.contains(1920.0 + 3840.0, 10.0));
        assert_eq!(m.to_local_logical(2920.0, 500.0), (500.0, 250.0));
    }

    #[test]
    fn handles_monitor_left_of_primary() {
        let m = MonitorRect {
            x: -2560,
            y: -200,
            width: 2560,
            height: 1440,
            scale: 1.25,
        };
        assert!(m.contains(-1.0, -200.0));
        assert_eq!(
            m.to_local_logical(-2560.0 + 125.0, -200.0 + 250.0),
            (100.0, 200.0)
        );
    }
}
