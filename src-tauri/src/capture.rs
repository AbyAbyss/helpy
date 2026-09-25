//! Screenshots of the monitor under the cursor, sized for a model, with the
//! numbers needed to map model coordinates back to the screen.

use std::io::Cursor;

use base64::Engine;
use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use ts_rs::TS;

use crate::ai::error::{ErrorKind, ProviderError};
use crate::overlay::{MonitorRect, Overlays};
use crate::settings::SettingsStore;

/// Longest edge sent to a model. Larger screens are scaled down.
pub const MAX_EDGE: u32 = 1568;
const THUMB_WIDTH: u32 = 360;

/// Where a screenshot came from and how it was scaled.
#[derive(Serialize, TS, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CaptureMeta {
    /// Monitor bounds in physical pixels.
    pub monitor_x: i32,
    pub monitor_y: i32,
    pub monitor_width: u32,
    pub monitor_height: u32,
    pub scale_factor: f64,
    /// Size of the image the model sees.
    pub image_width: u32,
    pub image_height: u32,
}

impl CaptureMeta {
    fn new(m: &MonitorRect, image_width: u32, image_height: u32) -> Self {
        Self {
            monitor_x: m.x,
            monitor_y: m.y,
            monitor_width: m.width,
            monitor_height: m.height,
            scale_factor: m.scale,
            image_width,
            image_height,
        }
    }

    /// A point in model-image pixels → global physical screen pixels.
    pub fn to_global_physical(self, x: f64, y: f64) -> (f64, f64) {
        (
            self.monitor_x as f64 + x * self.monitor_width as f64 / self.image_width as f64,
            self.monitor_y as f64 + y * self.monitor_height as f64 / self.image_height as f64,
        )
    }

    /// A point in model-image pixels → the overlay's CSS pixels on that monitor.
    pub fn to_overlay(self, x: f64, y: f64) -> (f64, f64) {
        let (gx, gy) = self.to_global_physical(x, y);
        (
            (gx - self.monitor_x as f64) / self.scale_factor,
            (gy - self.monitor_y as f64) / self.scale_factor,
        )
    }
}

pub struct Shot {
    pub jpeg_base64: String,
    pub thumbnail_data_url: String,
    pub meta: CaptureMeta,
    pub monitor_name: String,
}

/// Size that fits within MAX_EDGE, keeping the aspect ratio.
pub fn fit(width: u32, height: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= MAX_EDGE {
        return (width, height);
    }
    let s = MAX_EDGE as f64 / long as f64;
    (
        ((width as f64 * s).round() as u32).max(1),
        ((height as f64 * s).round() as u32).max(1),
    )
}

pub fn jpeg(img: &DynamicImage, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(Cursor::new(&mut out), quality);
    img.to_rgb8()
        .write_with_encoder(encoder)
        .expect("JPEG encoding of an in-memory image");
    out
}

pub fn encode(raw: RgbaImage, monitor: &MonitorRect) -> (String, String, CaptureMeta) {
    let (w, h) = fit(raw.width(), raw.height());
    let img = DynamicImage::ImageRgba8(raw);
    let sized = if (w, h) == (img.width(), img.height()) {
        img.clone()
    } else {
        img.resize_exact(w, h, FilterType::Triangle)
    };
    let b64 = base64::engine::general_purpose::STANDARD;
    let full = b64.encode(jpeg(&sized, 82));
    let thumb = sized.resize(THUMB_WIDTH, THUMB_WIDTH * 4, FilterType::Triangle);
    let thumb_url = format!("data:image/jpeg;base64,{}", b64.encode(jpeg(&thumb, 70)));
    (full, thumb_url, CaptureMeta::new(monitor, w, h))
}

fn paused() -> ProviderError {
    ProviderError::new(
        ErrorKind::Setup,
        "Screen capture is paused. Resume it from the tray menu",
    )
}

/// A full-resolution capture of one monitor.
pub struct Frame {
    pub image: RgbaImage,
    pub monitor: MonitorRect,
    pub name: String,
}

/// Captures the monitor the cursor is on. Helpy's own windows are left out:
/// Windows and macOS exclude them from capture; on Linux they're hidden for
/// the moment of the capture.
pub async fn capture_cursor_monitor(app: &AppHandle) -> Result<Shot, ProviderError> {
    let frame = grab_cursor_monitor(app).await?;
    let monitor = frame.monitor;
    let (jpeg_base64, thumbnail_data_url, meta) =
        tokio::task::spawn_blocking(move || encode(frame.image, &monitor))
            .await
            .map_err(|e| ProviderError::new(ErrorKind::Setup, e.to_string()))?;
    Ok(Shot {
        jpeg_base64,
        thumbnail_data_url,
        meta,
        monitor_name: frame.name,
    })
}

/// The cursor's monitor at full resolution, for cropping.
pub async fn grab_cursor_monitor(app: &AppHandle) -> Result<Frame, ProviderError> {
    if app.state::<SettingsStore>().get().privacy.capture_paused {
        return Err(paused());
    }
    let pos = app.cursor_position().map_err(|e| {
        ProviderError::new(ErrorKind::Setup, format!("Couldn't find the cursor: {e}"))
    })?;
    let monitors = app.state::<Overlays>().snapshot();
    let monitor = monitors
        .iter()
        .map(|(_, m)| *m)
        .find(|m| m.contains(pos.x, pos.y))
        .or_else(|| monitors.first().map(|(_, m)| *m))
        .ok_or_else(|| ProviderError::new(ErrorKind::Setup, "No monitor found"))?;

    #[cfg(target_os = "linux")]
    let hidden = hide_own_windows(app).await;

    let result = tokio::task::spawn_blocking(move || grab(&monitor)).await;

    #[cfg(target_os = "linux")]
    restore_own_windows(hidden);

    let (image, name) =
        result.map_err(|e| ProviderError::new(ErrorKind::Setup, e.to_string()))??;
    Ok(Frame {
        image,
        monitor,
        name,
    })
}

/// Grabs the xcap monitor matching our monitor rect.
fn grab(target: &MonitorRect) -> Result<(RgbaImage, String), ProviderError> {
    let fail = |e: &dyn std::fmt::Display| {
        ProviderError::new(
            ErrorKind::Setup,
            format!("Couldn't capture the screen: {e}. On macOS, allow Helpy under Privacy & Security → Screen Recording"),
        )
    };
    let monitors = xcap::Monitor::all().map_err(|e| fail(&e))?;
    // xcap reports logical points on macOS and physical pixels elsewhere;
    // compare both ways and take the closest match.
    let centre = (
        target.x as f64 + target.width as f64 / 2.0,
        target.y as f64 + target.height as f64 / 2.0,
    );
    let distance = |m: &xcap::Monitor| -> f64 {
        let (Ok(x), Ok(y), Ok(w), Ok(h), Ok(s)) =
            (m.x(), m.y(), m.width(), m.height(), m.scale_factor())
        else {
            return f64::MAX;
        };
        [1.0, s as f64]
            .iter()
            .map(|k| {
                let cx = (x as f64 + w as f64 / 2.0) * k;
                let cy = (y as f64 + h as f64 / 2.0) * k;
                (cx - centre.0).abs() + (cy - centre.1).abs()
            })
            .fold(f64::MAX, f64::min)
    };
    let monitor = monitors
        .into_iter()
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))
        .ok_or_else(|| fail(&"no monitors"))?;
    let name = monitor.name().unwrap_or_else(|_| "Display".into());
    let image = monitor.capture_image().map_err(|e| fail(&e))?;
    Ok((image, name))
}

#[cfg(target_os = "linux")]
async fn hide_own_windows(app: &AppHandle) -> Vec<tauri::WebviewWindow> {
    let hidden: Vec<_> = app
        .webview_windows()
        .into_values()
        .filter(|w| {
            w.label().starts_with("overlay-")
                || w.label() == crate::windows::ASK
                || w.label() == crate::windows::PILL
                || w.label() == crate::windows::STEP
        })
        .filter(|w| w.is_visible().unwrap_or(false))
        .collect();
    for w in &hidden {
        let _ = w.hide();
    }
    // Give the compositor a frame to repaint without them.
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    hidden
}

#[cfg(target_os = "linux")]
fn restore_own_windows(mut windows: Vec<tauri::WebviewWindow>) {
    // Overlays first, so the panel and step card are mapped last and stay on top.
    windows.sort_by_key(|w| !w.label().starts_with("overlay-"));
    for w in windows {
        let _ = w.show();
        if w.label().starts_with("overlay-") {
            let _ = w.set_ignore_cursor_events(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_long_edge_and_keeps_aspect() {
        assert_eq!(fit(3840, 2160), (1568, 882));
        assert_eq!(fit(1080, 1920), (882, 1568));
        assert_eq!(fit(1280, 720), (1280, 720));
    }

    #[test]
    fn maps_model_coordinates_back_through_resize_and_dpi() {
        // A 4K monitor at 200% to the right of a 1080p one, sent at 1568x882.
        let m = MonitorRect {
            x: 1920,
            y: 0,
            width: 3840,
            height: 2160,
            scale: 2.0,
        };
        let meta = CaptureMeta::new(&m, 1568, 882);
        let (gx, gy) = meta.to_global_physical(784.0, 441.0);
        assert!((gx - (1920.0 + 1920.0)).abs() < 0.01 && (gy - 1080.0).abs() < 0.01);
        let (ox, oy) = meta.to_overlay(784.0, 441.0);
        assert!((ox - 960.0).abs() < 0.01 && (oy - 540.0).abs() < 0.01);
        // Corners map to corners.
        assert_eq!(meta.to_global_physical(0.0, 0.0), (1920.0, 0.0));
        assert_eq!(
            meta.to_global_physical(1568.0, 882.0),
            (1920.0 + 3840.0, 2160.0)
        );
    }

    #[test]
    fn encodes_a_downscaled_jpeg_and_thumbnail() {
        let m = MonitorRect {
            x: 0,
            y: 0,
            width: 2000,
            height: 1000,
            scale: 1.0,
        };
        let (full, thumb, meta) = encode(
            RgbaImage::from_pixel(2000, 1000, image::Rgba([200, 30, 40, 255])),
            &m,
        );
        assert_eq!((meta.image_width, meta.image_height), (1568, 784));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(full)
            .unwrap();
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!((back.width(), back.height()), (1568, 784));
        assert!(thumb.starts_with("data:image/jpeg;base64,"));
    }
}
