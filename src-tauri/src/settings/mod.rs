//! Settings: schema, validation, persistence, and the commands the settings window uses.

pub mod schema;
pub mod store;
pub mod validate;

use std::fs;
use std::path::PathBuf;

use base64::Engine;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use ts_rs::TS;

use crate::state::AppState;
use schema::{BuddyStyle, SectionId, Settings};
use store::StoreError;
use validate::FieldError;

pub const CHANGED_EVENT: &str = "settings://changed";

/// Errors sent to the frontend. `invalid` carries per-field messages for inline display.
#[derive(Serialize, Debug, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum CommandError {
    Invalid { errors: Vec<FieldError> },
    Message { message: String },
}

impl From<StoreError> for CommandError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Invalid(errors) => CommandError::Invalid { errors },
            other => CommandError::Message { message: other.to_string() },
        }
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        CommandError::Message { message: e.to_string() }
    }
}

fn msg(m: impl Into<String>) -> CommandError {
    CommandError::Message { message: m.into() }
}

/// Saves, swaps in, broadcasts, and lets each subsystem react to what changed.
pub fn commit(app: &AppHandle, next: Settings) -> Result<Settings, CommandError> {
    let state = app.state::<AppState>();
    store::save(&state.settings_path, &next)?;
    let prev = {
        let mut guard = state.settings.write().expect("settings lock");
        std::mem::replace(&mut *guard, next.clone())
    };
    let _ = app.emit(CHANGED_EVENT, &next);
    on_changed(app, &prev, &next);
    Ok(next)
}

fn on_changed(app: &AppHandle, prev: &Settings, next: &Settings) {
    if prev.hotkeys != next.hotkeys {
        crate::hotkeys::sync(app);
    }
    if prev.general.launch_at_login != next.general.launch_at_login {
        sync_autostart(app, next.general.launch_at_login);
    }
    if prev.general.theme != next.general.theme {
        crate::app_windows::apply_theme(app);
    }
    crate::tray::sync(app);
}

pub fn sync_autostart(app: &AppHandle, enabled: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let al = app.autolaunch();
    let result = if enabled { al.enable() } else { al.disable() };
    if let Err(e) = result {
        log::warn!("could not change launch at login: {e}");
    }
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn update_settings(app: AppHandle, state: State<AppState>, patch: Value) -> Result<Settings, CommandError> {
    let next = store::apply_patch(&state.settings(), patch)?;
    commit(&app, next)
}

#[tauri::command]
pub fn reset_settings_section(app: AppHandle, state: State<AppState>, section: SectionId) -> Result<Settings, CommandError> {
    let mut next = state.settings();
    store::reset_section(&mut next, section);
    commit(&app, next)
}

#[tauri::command]
pub fn reset_all_settings(app: AppHandle, state: State<AppState>) -> Result<Settings, CommandError> {
    let mut next = Settings::default();
    next.buddy.custom_image = state.settings().buddy.custom_image;
    commit(&app, next)
}

/// Writes the current settings to `path`. Settings never hold secrets, so this is safe to share.
#[tauri::command]
pub fn export_settings(state: State<AppState>, path: PathBuf) -> Result<(), CommandError> {
    let mut s = state.settings();
    s.buddy.custom_image = None;
    let text = serde_json::to_string_pretty(&s).map_err(|e| msg(e.to_string()))?;
    fs::write(path, text)?;
    Ok(())
}

#[tauri::command]
pub fn import_settings(app: AppHandle, state: State<AppState>, path: PathBuf) -> Result<Settings, CommandError> {
    let meta = fs::metadata(&path)?;
    if meta.len() > 1_000_000 {
        return Err(msg("That file is larger than 1 MB, so it can't be a Helpy settings export."));
    }
    let text = fs::read_to_string(&path)?;
    let next = store::parse_import(&text, &state.settings())?;
    commit(&app, next)
}

const MAX_BUDDY_BYTES: u64 = 2 * 1024 * 1024;

/// Copies a user-chosen SVG or PNG into app data and switches the buddy to it.
#[tauri::command]
pub fn set_custom_buddy(app: AppHandle, state: State<AppState>, path: PathBuf) -> Result<Settings, CommandError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if ext != "svg" && ext != "png" {
        return Err(msg("Choose an .svg or .png file."));
    }
    if fs::metadata(&path)?.len() > MAX_BUDDY_BYTES {
        return Err(msg("Buddy images must be 2 MB or smaller."));
    }
    let bytes = fs::read(&path)?;
    let looks_right = match ext.as_str() {
        "png" => bytes.starts_with(&[0x89, b'P', b'N', b'G']),
        _ => String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).contains("<svg"),
    };
    if !looks_right {
        return Err(msg(format!("That file doesn't look like a valid {}.", ext.to_uppercase())));
    }
    let dir = state.data_dir.join("buddy");
    fs::create_dir_all(&dir)?;
    // Remove an older upload with the other extension.
    for old in ["custom.svg", "custom.png"] {
        let _ = fs::remove_file(dir.join(old));
    }
    let name = format!("custom.{ext}");
    fs::write(dir.join(&name), bytes)?;

    let mut next = state.settings();
    next.buddy.custom_image = Some(name);
    next.buddy.style = BuddyStyle::Custom;
    commit(&app, next)
}

/// Returns the uploaded buddy as a data URL. Rendered with <img>, so SVG scripts never run.
#[tauri::command]
pub fn get_custom_buddy(state: State<AppState>) -> Option<String> {
    let name = state.settings().buddy.custom_image?;
    let bytes = fs::read(state.data_dir.join("buddy").join(&name)).ok()?;
    let mime = if name.ends_with(".png") { "image/png" } else { "image/svg+xml" };
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod export_bindings {
    use super::*;

    /// `cargo test` regenerates `src/bindings/*.ts` (ts-rs) and the defaults file here.
    #[test]
    fn export_default_settings() {
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings/defaultSettings.json");
        fs::create_dir_all(out.parent().unwrap()).unwrap();
        let text = serde_json::to_string_pretty(&Settings::default()).unwrap() + "\n";
        fs::write(out, text).unwrap();
    }
}
