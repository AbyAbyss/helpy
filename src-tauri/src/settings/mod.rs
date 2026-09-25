//! Settings persistence, validation and the commands the settings page calls.

pub mod profiles;
pub mod schema;
mod validate;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use ts_rs::TS;

pub use schema::Settings;
pub use validate::validate;

pub const CHANGED_EVENT: &str = "settings://changed";

/// A problem with one setting, addressed by its dotted path ("buddy.size").
#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FieldError {
    pub path: String,
    pub message: String,
}

impl FieldError {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
    profiles: profiles::Store,
}

impl SettingsStore {
    /// Loads settings from `path`. A missing file gives defaults. A file that
    /// can't be parsed is kept as `settings.broken.json` so nothing is lost,
    /// and defaults are used. Values that parse but fail validation are
    /// replaced by their defaults one section at a time.
    pub fn load(path: PathBuf) -> Self {
        let current = match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(s) => repair(s),
                Err(e) => {
                    log::warn!("settings file unreadable ({e}), using defaults");
                    let _ = fs::rename(&path, path.with_file_name("settings.broken.json"));
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        };
        Self {
            profiles: profiles::Store::load(&path),
            path,
            current: Mutex::new(current),
        }
    }

    pub fn get(&self) -> Settings {
        self.current.lock().unwrap().clone()
    }

    fn save(&self, settings: &Settings) -> std::io::Result<()> {
        write_atomic(&self.path, &serde_json::to_vec_pretty(settings)?)
    }
}

/// Resets any section that fails validation, so a hand-edited file with one
/// bad value doesn't take the rest of the settings down with it.
fn repair(mut s: Settings) -> Settings {
    let errors = validate(&s);
    if errors.is_empty() {
        return s;
    }
    let mut value = serde_json::to_value(&s).unwrap();
    let defaults = serde_json::to_value(Settings::default()).unwrap();
    for e in &errors {
        log::warn!(
            "invalid setting {}: {}, resetting its section",
            e.path,
            e.message
        );
        let section = e.path.split('.').next().unwrap_or_default();
        value[section] = defaults[section].clone();
    }
    s = serde_json::from_value(value).unwrap_or_default();
    s
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let mut f = fs::File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(tmp, path)
}

/// Validates, saves and broadcasts a new settings value, then lets the rest of
/// the app react (hotkeys, tray, autostart).
pub fn commit(app: &AppHandle, next: Settings) -> Result<Settings, Vec<FieldError>> {
    let store = app.state::<SettingsStore>();
    let prev = store.get();
    let mut saved = store.profiles.saved.lock().unwrap().clone();
    let next = profiles::apply(&prev, next, &mut saved);
    let errors = validate(&next);
    if !errors.is_empty() {
        return Err(errors);
    }
    if prev == next {
        return Ok(next);
    }
    store
        .save(&next)
        .map_err(|e| vec![FieldError::new("", format!("Couldn't save settings: {e}"))])?;
    *store.profiles.saved.lock().unwrap() = saved;
    store.profiles.save();
    *store.current.lock().unwrap() = next.clone();
    let _ = app.emit(CHANGED_EVENT, &next);
    crate::on_settings_changed(app, &prev, &next);
    Ok(next)
}

/// Replaces the value at a dotted camelCase path, for example "buddy.size".
pub fn with_value(
    settings: &Settings,
    path: &str,
    value: Value,
) -> Result<Settings, Vec<FieldError>> {
    let mut root = serde_json::to_value(settings).unwrap();
    let mut slot = &mut root;
    for key in path.split('.') {
        slot = slot
            .get_mut(key)
            .ok_or_else(|| vec![FieldError::new(path, "Unknown setting")])?;
    }
    *slot = value;
    serde_json::from_value(root)
        .map_err(|e| vec![FieldError::new(path, format!("Wrong type of value: {e}"))])
}

#[tauri::command]
pub fn settings_get(store: State<SettingsStore>) -> Settings {
    store.get()
}

#[tauri::command]
pub fn settings_defaults() -> Settings {
    Settings::default()
}

#[tauri::command]
pub fn settings_set(
    app: AppHandle,
    path: String,
    value: Value,
) -> Result<Settings, Vec<FieldError>> {
    let next = with_value(&app.state::<SettingsStore>().get(), &path, value)?;
    commit(&app, next)
}

#[tauri::command]
pub fn settings_reset_section(
    app: AppHandle,
    section: String,
) -> Result<Settings, Vec<FieldError>> {
    if !schema::SECTIONS.contains(&section.as_str()) {
        return Err(vec![FieldError::new(section, "Unknown section")]);
    }
    let defaults = serde_json::to_value(Settings::default()).unwrap();
    let next = with_value(
        &app.state::<SettingsStore>().get(),
        &section,
        defaults[&section].clone(),
    )?;
    commit(&app, next)
}

#[tauri::command]
pub fn settings_reset_all(app: AppHandle) -> Result<Settings, Vec<FieldError>> {
    commit(&app, Settings::default())
}

#[tauri::command]
pub fn settings_export(store: State<SettingsStore>, path: String) -> Result<(), String> {
    // Settings never hold secrets: API keys and tokens live in the OS keychain.
    let bytes = serde_json::to_vec_pretty(&store.get()).map_err(|e| e.to_string())?;
    fs::write(&path, bytes).map_err(|e| format!("Couldn't write {path}: {e}"))
}

#[tauri::command]
pub fn settings_import(app: AppHandle, path: String) -> Result<Settings, Vec<FieldError>> {
    let text = fs::read_to_string(&path)
        .map_err(|e| vec![FieldError::new("", format!("Couldn't read {path}: {e}"))])?;
    let parsed: Settings = serde_json::from_str(&text).map_err(|e| {
        vec![FieldError::new(
            "",
            format!("This file isn't a Helpy settings file: {e}"),
        )]
    })?;
    commit(&app, parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Runs with the ts-rs exports (`npm run bindings`). The settings page
    /// test uses this file to check that every setting has a row.
    #[test]
    fn export_bindings_defaults() {
        let dir = std::env::var("TS_RS_EXPORT_DIR").unwrap_or_else(|_| "bindings".into());
        fs::create_dir_all(&dir).unwrap();
        let json = serde_json::to_string_pretty(&Settings::default()).unwrap();
        fs::write(Path::new(&dir).join("defaults.json"), json + "\n").unwrap();
        let keys = serde_json::to_string_pretty(&profiles::KEYS).unwrap();
        fs::write(Path::new(&dir).join("profileKeys.json"), keys + "\n").unwrap();
    }

    #[test]
    fn only_fresh_installs_get_the_welcome_tour() {
        assert!(!Settings::default().general.onboarded);
        let older: Settings = serde_json::from_str(r#"{"general": {"theme": "dark"}}"#).unwrap();
        assert!(older.general.onboarded);
    }

    #[test]
    fn with_value_sets_nested_field() {
        let s = with_value(&Settings::default(), "buddy.size", json!(48)).unwrap();
        assert_eq!(s.buddy.size, 48);
    }

    #[test]
    fn with_value_rejects_unknown_path_and_wrong_type() {
        assert_eq!(
            with_value(&Settings::default(), "buddy.nope", json!(1)).unwrap_err()[0].message,
            "Unknown setting"
        );
        assert!(with_value(&Settings::default(), "buddy.size", json!("big")).is_err());
    }

    #[test]
    fn partial_file_fills_defaults() {
        let s: Settings =
            serde_json::from_str(r#"{"buddy":{"size":50},"futureSection":{"x":1}}"#).unwrap();
        assert_eq!(s.buddy.size, 50);
        assert_eq!(s.buddy.offset_x, Settings::default().buddy.offset_x);
        assert_eq!(s.hotkeys, Settings::default().hotkeys);
    }

    #[test]
    fn repair_resets_only_the_bad_section() {
        let mut s = Settings::default();
        s.buddy.size = 9999;
        s.general.start_minimized = false;
        let fixed = repair(s);
        assert_eq!(fixed.buddy, Settings::default().buddy);
        assert!(!fixed.general.start_minimized);
    }

    #[test]
    fn load_keeps_unreadable_file() {
        let dir = std::env::temp_dir().join(format!("helpy-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(&path, "{not json").unwrap();
        let store = SettingsStore::load(path.clone());
        assert_eq!(store.get(), Settings::default());
        assert!(dir.join("settings.broken.json").exists());
        store.save(&store.get()).unwrap();
        assert!(SettingsStore::load(path).get() == Settings::default());
        let _ = fs::remove_dir_all(dir);
    }
}
