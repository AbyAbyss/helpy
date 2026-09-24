//! Custom buddy images. The uploaded file is copied into the app config
//! folder so the original can be moved or deleted.

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::{self, Settings};

pub const IMAGE_CHANGED_EVENT: &str = "buddy://image-changed";
const MAX_BYTES: u64 = 1024 * 1024;

fn stored_path(app: &AppHandle, ext: &str) -> tauri::Result<PathBuf> {
    Ok(app
        .path()
        .app_config_dir()?
        .join(format!("buddy-custom.{ext}")))
}

#[tauri::command]
pub fn buddy_set_custom_image(app: AppHandle, path: String) -> Result<Settings, String> {
    let src = Path::new(&path);
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if ext != "svg" && ext != "png" {
        return Err("Pick an SVG or PNG file".into());
    }
    let size = fs::metadata(src)
        .map_err(|e| format!("Couldn't read that file: {e}"))?
        .len();
    if size > MAX_BYTES {
        return Err(format!(
            "That file is {} KB. Keep it under 1024 KB",
            size / 1024
        ));
    }
    for old in ["svg", "png"] {
        if let Ok(p) = stored_path(&app, old) {
            let _ = fs::remove_file(p);
        }
    }
    let dest = stored_path(&app, &ext).map_err(|e| e.to_string())?;
    fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::copy(src, &dest).map_err(|e| format!("Couldn't copy the image: {e}"))?;
    let _ = app.emit(IMAGE_CHANGED_EVENT, ());
    settings::settings_set(app, "buddy.style".into(), "custom".into()).map_err(|e| {
        e.into_iter()
            .map(|e| e.message)
            .collect::<Vec<_>>()
            .join(", ")
    })
}

/// The custom image as a data URL, or None if none was uploaded.
#[tauri::command]
pub fn buddy_custom_image(app: AppHandle) -> Option<String> {
    for (ext, mime) in [("svg", "image/svg+xml"), ("png", "image/png")] {
        if let Ok(bytes) = fs::read(stored_path(&app, ext).ok()?) {
            return Some(format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ));
        }
    }
    None
}
