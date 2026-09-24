//! Loading, patching, and saving settings on disk.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::schema::{SectionId, Settings, SCHEMA_VERSION};
use super::validate::{validate, FieldError};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("The file isn't valid Helpy settings JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Some settings are invalid")]
    Invalid(Vec<FieldError>),
}

/// Reads settings from disk. A missing file gives defaults. A corrupt file is moved aside
/// (`settings.json.bad`) so the user keeps a copy, and defaults are used.
pub fn load(path: &Path) -> Settings {
    let Ok(text) = fs::read_to_string(path) else {
        return Settings::default();
    };
    match serde_json::from_str::<Settings>(&text) {
        Ok(mut s) => {
            // Values that fail validation (hand-edited file, older version) fall back per section.
            repair(&mut s);
            s.version = SCHEMA_VERSION;
            s
        }
        Err(e) => {
            log::warn!("settings file unreadable, using defaults: {e}");
            let _ = fs::rename(path, path.with_extension("json.bad"));
            Settings::default()
        }
    }
}

/// Resets any section that contains invalid values.
fn repair(s: &mut Settings) {
    let errors = validate(s);
    for e in &errors {
        let section = e.path.split('.').next().unwrap_or_default();
        log::warn!("invalid setting {} ({}), resetting its section", e.path, e.message);
        match section {
            "general" => reset_section(s, SectionId::General),
            "buddy" => reset_section(s, SectionId::Buddy),
            "hotkeys" => reset_section(s, SectionId::Hotkeys),
            "voiceOutput" => reset_section(s, SectionId::VoiceOutput),
            _ => {}
        }
    }
}

/// Writes atomically: temp file in the same folder, then rename over the old file.
pub fn save(path: &Path, s: &Settings) -> Result<(), StoreError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp: PathBuf = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(serde_json::to_string_pretty(s)?.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

/// Deep-merges `patch` into `base` (objects merge, everything else replaces).
fn merge(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                merge(b.entry(k).or_insert(Value::Null), v);
            }
        }
        (b, p) => *b = p,
    }
}

/// Applies a partial update and validates the result. Nothing changes if validation fails.
pub fn apply_patch(current: &Settings, patch: Value) -> Result<Settings, StoreError> {
    let mut v = serde_json::to_value(current)?;
    merge(&mut v, patch);
    let next: Settings = serde_json::from_value(v)?;
    let errors = validate(&next);
    if errors.is_empty() {
        Ok(next)
    } else {
        Err(StoreError::Invalid(errors))
    }
}

pub fn reset_section(s: &mut Settings, section: SectionId) {
    let d = Settings::default();
    match section {
        SectionId::General => s.general = d.general,
        SectionId::Buddy => {
            // Keep the uploaded image so the user can switch back to it.
            let img = s.buddy.custom_image.take();
            s.buddy = d.buddy;
            s.buddy.custom_image = img;
        }
        SectionId::Hotkeys => s.hotkeys = d.hotkeys,
        SectionId::VoiceOutput => s.voice_output = d.voice_output,
    }
}

/// Parses an exported file. Unknown keys are ignored, missing keys take defaults,
/// and the result must validate. The custom buddy image is machine-specific, so it is dropped.
pub fn parse_import(text: &str, current: &Settings) -> Result<Settings, StoreError> {
    let mut s: Settings = serde_json::from_str(text)?;
    s.version = SCHEMA_VERSION;
    s.buddy.custom_image = current.buddy.custom_image.clone();
    if s.buddy.style == super::schema::BuddyStyle::Custom && s.buddy.custom_image.is_none() {
        s.buddy.style = super::schema::BuddyStyle::Pip;
    }
    let errors = validate(&s);
    if errors.is_empty() {
        Ok(s)
    } else {
        Err(StoreError::Invalid(errors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn patch_merges_nested_fields() {
        let s = Settings::default();
        let next = apply_patch(&s, json!({ "buddy": { "size": 48 } })).unwrap();
        assert_eq!(next.buddy.size, 48);
        assert_eq!(next.buddy.opacity, s.buddy.opacity);
    }

    #[test]
    fn invalid_patch_changes_nothing() {
        let s = Settings::default();
        match apply_patch(&s, json!({ "buddy": { "size": 1 } })) {
            Err(StoreError::Invalid(e)) => assert_eq!(e[0].path, "buddy.size"),
            other => panic!("expected invalid, got {other:?}"),
        }
    }

    #[test]
    fn partial_files_fill_defaults() {
        let s: Settings = serde_json::from_str(r#"{ "buddy": { "size": 50 } }"#).unwrap();
        assert_eq!(s.buddy.size, 50);
        assert_eq!(s.hotkeys, Settings::default().hotkeys);
    }

    #[test]
    fn roundtrip_on_disk() {
        let dir = std::env::temp_dir().join(format!("helpy-test-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut s = Settings::default();
        s.general.theme = super::super::schema::ThemePreference::Dark;
        save(&path, &s).unwrap();
        assert_eq!(load(&path), s);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_file_is_kept_aside() {
        let dir = std::env::temp_dir().join(format!("helpy-bad-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path), Settings::default());
        assert!(dir.join("settings.json.bad").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn import_rejects_invalid_values() {
        let text = r#"{ "buddy": { "opacity": 9 } }"#;
        assert!(matches!(parse_import(text, &Settings::default()), Err(StoreError::Invalid(_))));
    }
}
