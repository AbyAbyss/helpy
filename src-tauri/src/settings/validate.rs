use std::collections::HashMap;
use std::str::FromStr;

use serde::Serialize;
use tauri_plugin_global_shortcut::Shortcut;
use ts_rs::TS;

use super::schema::{BuddyStyle, HotkeyAction, Settings};

/// One problem with one setting. `path` is the dotted camelCase path, e.g. `buddy.size`.
#[derive(Serialize, Clone, Debug, PartialEq, TS)]
#[ts(export)]
pub struct FieldError {
    pub path: String,
    pub message: String,
}

fn err(path: &str, message: impl Into<String>) -> FieldError {
    FieldError { path: path.into(), message: message.into() }
}

fn range<T: PartialOrd + Copy + std::fmt::Display>(
    out: &mut Vec<FieldError>,
    path: &str,
    v: T,
    min: T,
    max: T,
) {
    if v < min || v > max {
        out.push(err(path, format!("Use a value from {min} to {max}.")));
    }
}

fn language_tag(out: &mut Vec<FieldError>, path: &str, v: &str, allow_auto: bool) {
    if allow_auto && v == "auto" {
        return;
    }
    let ok = !v.is_empty()
        && v.len() <= 35
        && v.split('-').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()));
    if !ok {
        out.push(err(path, "Use a language code such as en, de, or pt-BR."));
    }
}

/// Parses an accelerator the same way the global shortcut plugin will.
pub fn parse_hotkey(s: &str) -> Result<Shortcut, String> {
    Shortcut::from_str(s).map_err(|_| format!("\"{s}\" isn't a key combination Helpy understands."))
}

pub fn validate(s: &Settings) -> Vec<FieldError> {
    let mut out = Vec::new();

    language_tag(&mut out, "general.interfaceLanguage", &s.general.interface_language, false);
    language_tag(&mut out, "general.responseLanguage", &s.general.response_language, true);

    let b = &s.buddy;
    range(&mut out, "buddy.size", b.size, 24, 96);
    range(&mut out, "buddy.opacity", b.opacity, 0.2, 1.0);
    range(&mut out, "buddy.offsetX", b.offset_x, -120, 120);
    range(&mut out, "buddy.offsetY", b.offset_y, -120, 120);
    range(&mut out, "buddy.smoothness", b.smoothness, 0.0, 1.0);
    range(&mut out, "buddy.idleSeconds", b.idle_seconds, 3, 600);
    if b.style == BuddyStyle::Custom && b.custom_image.is_none() {
        out.push(err("buddy.style", "Upload an SVG or PNG before choosing the custom style."));
    }

    // Hotkeys: each must parse, and no two actions may share a combination.
    let mut seen: HashMap<u32, HotkeyAction> = HashMap::new();
    for action in HotkeyAction::ALL {
        let raw = s.hotkeys.get(action);
        if raw.is_empty() {
            continue;
        }
        let path = format!("hotkeys.{}", action.key());
        match parse_hotkey(raw) {
            Ok(sc) => {
                if let Some(other) = seen.insert(sc.id(), action) {
                    out.push(err(&path, format!("Already used by \"{}\".", label(other))));
                }
            }
            Err(e) => out.push(err(&path, e)),
        }
    }

    out
}

pub fn label(a: HotkeyAction) -> &'static str {
    match a {
        HotkeyAction::VoiceAsk => "Voice ask",
        HotkeyAction::TextAsk => "Text ask",
        HotkeyAction::CircleToExplain => "Circle to explain",
        HotkeyAction::ClearAnnotations => "Clear annotations",
        HotkeyAction::PauseCapture => "Pause screen capture",
        HotkeyAction::OpenAgentPanel => "Open agent panel",
        HotkeyAction::OpenApprovalInbox => "Open approval inbox",
        HotkeyAction::PauseAllAgents => "Pause all agents",
        HotkeyAction::OpenSettings => "Open settings",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert_eq!(validate(&Settings::default()), vec![]);
    }

    #[test]
    fn rejects_out_of_range_and_duplicates() {
        let mut s = Settings::default();
        s.buddy.size = 500;
        s.hotkeys.text_ask = s.hotkeys.voice_ask.clone();
        s.hotkeys.open_settings = "Shift+Banana".into();
        let paths: Vec<_> = validate(&s).into_iter().map(|e| e.path).collect();
        assert_eq!(paths, vec!["buddy.size", "hotkeys.textAsk", "hotkeys.openSettings"]);
    }

    #[test]
    fn duplicate_detection_ignores_modifier_order() {
        let mut s = Settings::default();
        s.hotkeys.text_ask = "Shift+Alt+H".into();
        assert!(validate(&s).iter().any(|e| e.path == "hotkeys.textAsk"));
    }

    #[test]
    fn custom_style_needs_an_image() {
        let mut s = Settings::default();
        s.buddy.style = BuddyStyle::Custom;
        assert_eq!(validate(&s)[0].path, "buddy.style");
    }
}
