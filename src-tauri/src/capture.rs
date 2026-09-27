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
use crate::settings::schema::{Privacy, ProviderKind};
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

    /// The reverse of `to_global_physical`.
    pub fn from_global_physical(self, x: f64, y: f64) -> (f64, f64) {
        (
            (x - self.monitor_x as f64) * self.image_width as f64 / self.monitor_width as f64,
            (y - self.monitor_y as f64) * self.image_height as f64 / self.monitor_height as f64,
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
    ImageLimit::DEFAULT.fit(width, height)
}

/// The largest screenshot a model reads without its provider shrinking it
/// first. Staying inside it keeps the pixel size Helpy states the size the
/// model sees, so its coordinates land where it means.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageLimit {
    pub long_edge: u32,
    pub short_edge: u32,
    pub pixels: u32,
}

impl ImageLimit {
    pub const DEFAULT: Self = Self {
        long_edge: MAX_EDGE,
        short_edge: MAX_EDGE,
        pixels: u32::MAX,
    };

    pub fn for_model(kind: ProviderKind, model: &str) -> Self {
        // Claude models from Opus 4.7 on read up to 2576 px and 4784 image
        // tokens (28x28 px each) at full size; earlier ones 1568 px and about
        // 1.15 megapixels.
        const HIGH_RES: [&str; 6] = ["opus-4-7", "opus-4-8", "opus-5", "sonnet-5", "fable", "mythos"];
        match kind {
            ProviderKind::Anthropic if HIGH_RES.iter().any(|m| model.contains(m)) => Self {
                long_edge: 2576,
                short_edge: 2576,
                pixels: 4784 * 784,
            },
            ProviderKind::Anthropic => Self {
                long_edge: 1568,
                short_edge: 1568,
                pixels: 1_150_000,
            },
            // OpenAI fits images in 2048 px, then brings the short side to 768.
            ProviderKind::OpenAi => Self {
                long_edge: 2048,
                short_edge: 768,
                pixels: u32::MAX,
            },
            _ => Self::DEFAULT,
        }
    }

    /// Size within the limit, keeping the aspect ratio.
    pub fn fit(self, width: u32, height: u32) -> (u32, u32) {
        let (long, short) = (width.max(height) as f64, width.min(height) as f64);
        let s = (self.long_edge as f64 / long)
            .min(self.short_edge as f64 / short)
            .min((self.pixels as f64 / (long * short)).sqrt());
        if s >= 1.0 {
            return (width, height);
        }
        // Rounded down, so the result never goes over a limit.
        let size = |v: u32| ((v as f64 * s + 1e-6).floor() as u32).max(1);
        (size(width), size(height))
    }
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

pub fn encode(
    raw: RgbaImage,
    monitor: &MonitorRect,
    limit: ImageLimit,
) -> (String, String, CaptureMeta) {
    let (w, h) = limit.fit(raw.width(), raw.height());
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
pub async fn capture_cursor_monitor(
    app: &AppHandle,
    limit: ImageLimit,
) -> Result<Shot, ProviderError> {
    let frame = grab_cursor_monitor(app).await?;
    let monitor = frame.monitor;
    let (jpeg_base64, thumbnail_data_url, meta) =
        tokio::task::spawn_blocking(move || encode(frame.image, &monitor, limit))
            .await
            .map_err(|e| ProviderError::new(ErrorKind::Setup, e.to_string()))?;
    Ok(Shot {
        jpeg_base64,
        thumbnail_data_url,
        meta,
        monitor_name: frame.name,
    })
}

/// Makes sure Helpy may record the screen. Without it macOS still hands
/// over a capture, but with only the wallpaper, so it's checked first. macOS
/// shows its own prompt only while Helpy has no answer on record; after one
/// was denied or removed, the settings page opens instead, once per launch.
#[cfg(target_os = "macos")]
fn check_screen_permission() -> Result<(), ProviderError> {
    use std::sync::atomic::{AtomicBool, Ordering};
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    static OPENED_SETTINGS: AtomicBool = AtomicBool::new(false);
    // Both only read the permission as it was when Helpy started.
    if unsafe { CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess() } {
        return Ok(());
    }
    if !OPENED_SETTINGS.swap(true, Ordering::SeqCst) {
        let _ = crate::permissions::permissions_open("screen".into());
    }
    Err(ProviderError::new(
        ErrorKind::Setup,
        "Helpy doesn't have Screen Recording permission. Turn Helpy on in System Settings → Privacy & \
         Security → Screen Recording (remove it with – and add it again if it's already on), then quit \
         and reopen Helpy",
    ))
}

/// The cursor's monitor at full resolution, for cropping.
pub async fn grab_cursor_monitor(app: &AppHandle) -> Result<Frame, ProviderError> {
    let privacy = app.state::<SettingsStore>().get().privacy;
    if privacy.capture_paused {
        return Err(paused());
    }
    #[cfg(target_os = "macos")]
    check_screen_permission()?;
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

    let blur_passwords = privacy.blur_passwords;
    let result = tokio::task::spawn_blocking(move || grab(&monitor, &privacy)).await;

    #[cfg(target_os = "linux")]
    restore_own_windows(hidden);

    let (mut image, name) =
        result.map_err(|e| ProviderError::new(ErrorKind::Setup, e.to_string()))??;
    if blur_passwords {
        let k = image.width() as f64 / monitor.width as f64;
        let rects: Vec<_> = crate::a11y::password_fields(monitor.scale)
            .await
            .into_iter()
            .map(|r| {
                let (x, y) = (r.x - monitor.x as f64, r.y - monitor.y as f64);
                (x * k - 2.0, y * k - 2.0, r.w * k + 4.0, r.h * k + 4.0)
            })
            .collect();
        crate::privacy::blank(&mut image, &rects);
    }
    Ok(Frame {
        image,
        monitor,
        name,
    })
}

/// Grabs the xcap monitor matching our monitor rect.
fn grab(target: &MonitorRect, privacy: &Privacy) -> Result<(RgbaImage, String), ProviderError> {
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
    let mut image = monitor.capture_image().map_err(|e| fail(&e))?;
    let (Ok(x), Ok(y), Ok(w), Ok(h)) =
        (monitor.x(), monitor.y(), monitor.width(), monitor.height())
    else {
        return Err(fail(&"the monitor's position is unknown"));
    };
    crate::privacy::guard(
        &mut image,
        (x as f64, y as f64, w as f64, h as f64),
        privacy,
    )
    .map_err(|why| ProviderError::new(ErrorKind::Setup, why))?;
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
    fn sizes_screenshots_for_the_model_that_reads_them() {
        // A 14" MacBook Pro screen, 3024x1964 physical pixels.
        let size = |kind, model| ImageLimit::for_model(kind, model).fit(3024, 1964);
        // Current Claude: the 4784-token cap binds before 2576 px.
        let (w, h) = size(ProviderKind::Anthropic, "claude-sonnet-5");
        assert_eq!((w, h), (2403, 1560));
        assert!(w * h <= 4784 * 784);
        assert_eq!(size(ProviderKind::Anthropic, "claude-opus-5-5"), (2403, 1560));
        // Earlier Claude: about 1.15 megapixels.
        let (w, h) = size(ProviderKind::Anthropic, "claude-sonnet-4-5");
        assert_eq!((w, h), (1330, 864));
        // OpenAI: the short side at 768, as OpenAI would scale it.
        assert_eq!(size(ProviderKind::OpenAi, "gpt-4.1-mini"), (1182, 768));
        // Everything else keeps the old 1568 px edge.
        assert_eq!(size(ProviderKind::Gemini, "gemini-2.5-pro"), (1568, 1018));
        // Small screens are never enlarged.
        let claude = ImageLimit::for_model(ProviderKind::Anthropic, "claude-sonnet-5");
        assert_eq!(claude.fit(1280, 800), (1280, 800));
        assert_eq!(ImageLimit::for_model(ProviderKind::OpenAi, "x").fit(1024, 640), (1024, 640));
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
            ImageLimit::DEFAULT,
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
