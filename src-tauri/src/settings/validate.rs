use super::schema::Settings;
use super::FieldError;
use crate::hotkeys;

const INTERFACE_LANGUAGES: &[&str] = &["en"];

pub fn validate(s: &Settings) -> Vec<FieldError> {
    let mut errors = Vec::new();
    let mut range = |path: &str, v: f64, min: f64, max: f64| {
        if !(min..=max).contains(&v) {
            errors.push(FieldError::new(
                path,
                format!("Must be between {min} and {max}"),
            ));
        }
    };

    let b = &s.buddy;
    range("buddy.size", b.size as f64, 16.0, 128.0);
    range("buddy.opacity", b.opacity, 0.2, 1.0);
    range("buddy.offsetX", b.offset_x as f64, -200.0, 200.0);
    range("buddy.offsetY", b.offset_y as f64, -200.0, 200.0);
    range("buddy.smoothness", b.smoothness, 0.0, 0.95);
    range("buddy.idleSeconds", b.idle_seconds as f64, 2.0, 3600.0);

    if !INTERFACE_LANGUAGES.contains(&s.general.interface_language.as_str()) {
        errors.push(FieldError::new(
            "general.interfaceLanguage",
            "Only English is available right now",
        ));
    }
    if !is_language_tag(&s.general.response_language) {
        errors.push(FieldError::new(
            "general.responseLanguage",
            "Use \"auto\" or a language code such as en, de or pt-BR",
        ));
    }

    let mut seen: Vec<(&str, u32)> = Vec::new();
    for (key, label, accel) in s.hotkeys.actions() {
        if accel.is_empty() {
            continue;
        }
        let path = format!("hotkeys.{key}");
        match hotkeys::parse(accel) {
            Err(message) => errors.push(FieldError::new(path, message)),
            Ok(shortcut) => {
                if let Some((other, _)) = seen.iter().find(|(_, id)| *id == shortcut.id()) {
                    errors.push(FieldError::new(path, format!("Already used by {other}")));
                } else {
                    seen.push((label, shortcut.id()));
                }
            }
        }
    }
    errors
}

/// "auto", or a simple BCP 47 tag: 2-3 letter language, optional region/script.
fn is_language_tag(tag: &str) -> bool {
    if tag == "auto" {
        return true;
    }
    let mut parts = tag.split('-');
    let lang_ok = parts
        .next()
        .is_some_and(|l| (2..=3).contains(&l.len()) && l.chars().all(|c| c.is_ascii_lowercase()));
    lang_ok
        && parts.all(|p| (2..=4).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert_eq!(validate(&Settings::default()), vec![]);
    }

    #[test]
    fn out_of_range_values_are_reported_by_path() {
        let mut s = Settings::default();
        s.buddy.size = 4;
        s.buddy.opacity = 0.0;
        let paths: Vec<_> = validate(&s).into_iter().map(|e| e.path).collect();
        assert_eq!(paths, ["buddy.size", "buddy.opacity"]);
    }

    #[test]
    fn duplicate_hotkeys_name_the_other_action() {
        let mut s = Settings::default();
        s.hotkeys.text_ask = s.hotkeys.voice_ask.clone();
        let e = validate(&s);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].path, "hotkeys.textAsk");
        assert_eq!(e[0].message, "Already used by Voice ask");
    }

    #[test]
    fn duplicate_detection_ignores_modifier_order_and_case() {
        let mut s = Settings::default();
        s.hotkeys.text_ask = "shift+alt+space".into();
        assert_eq!(validate(&s)[0].path, "hotkeys.textAsk");
    }

    #[test]
    fn unparseable_hotkey_is_an_error_and_empty_is_unbound() {
        let mut s = Settings::default();
        s.hotkeys.text_ask = "Alt+Banana".into();
        s.hotkeys.circle_to_explain = String::new();
        let e = validate(&s);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].path, "hotkeys.textAsk");
    }

    #[test]
    fn language_tags() {
        for ok in ["auto", "en", "de", "pt-BR", "zh-Hant"] {
            assert!(is_language_tag(ok), "{ok}");
        }
        for bad in ["", "English", "EN", "e", "en_US", "en-"] {
            assert!(!is_language_tag(bad), "{bad}");
        }
    }
}
