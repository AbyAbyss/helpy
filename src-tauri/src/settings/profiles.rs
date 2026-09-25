//! Behavior profiles. Each profile keeps its own values for a few settings
//! (how much Helpy talks, how much it explains, how much it moves).
//! Switching profiles saves the current values under the old profile and
//! loads the new one's; changing one of these settings changes it only for
//! the active profile.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Value};

use super::schema::{BuiltinProfile, Settings};

/// The settings each profile keeps its own value of.
pub const KEYS: [&str; 9] = [
    "answerStyle.detail",
    "voiceOutput.voiceGuidance",
    "voiceOutput.readAloud",
    "voiceOutput.announceAgents",
    "circle.labelDetail",
    "guidance.reduceMotion",
    "buddy.showStateAnimations",
    "agents.speakStatus",
    "agents.notifications",
];

/// A profile's values the first time it's used.
fn preset(p: BuiltinProfile) -> Vec<(&'static str, Value)> {
    match p {
        // Beginner is how Helpy starts, so it's the defaults.
        BuiltinProfile::Beginner => {
            let d = serde_json::to_value(Settings::default()).unwrap();
            KEYS.iter().map(|k| (*k, get(&d, k).clone())).collect()
        }
        BuiltinProfile::Expert => vec![
            ("answerStyle.detail", json!("brief")),
            ("voiceOutput.voiceGuidance", json!(false)),
            ("voiceOutput.readAloud", json!("stepsOnly")),
            ("voiceOutput.announceAgents", json!(false)),
            ("circle.labelDetail", json!("names")),
            ("guidance.reduceMotion", json!(false)),
            ("buddy.showStateAnimations", json!(true)),
            ("agents.speakStatus", json!(false)),
            ("agents.notifications", json!(true)),
        ],
        BuiltinProfile::Quiet => vec![
            ("answerStyle.detail", json!("normal")),
            ("voiceOutput.voiceGuidance", json!(false)),
            ("voiceOutput.readAloud", json!("stepsOnly")),
            ("voiceOutput.announceAgents", json!(false)),
            ("circle.labelDetail", json!("names")),
            ("guidance.reduceMotion", json!(true)),
            ("buddy.showStateAnimations", json!(false)),
            ("agents.speakStatus", json!(false)),
            ("agents.notifications", json!(false)),
        ],
    }
}

fn get<'a>(v: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(v, |v, k| &v[k])
}

fn set(v: &mut Value, path: &str, value: Value) {
    let slot = path.split('.').fold(v, |v, k| &mut v[k]);
    *slot = value;
}

fn name(p: BuiltinProfile) -> String {
    serde_json::to_value(p)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

/// Each profile's saved values, by profile then setting path.
pub type Saved = BTreeMap<String, BTreeMap<String, Value>>;

/// The settings to save when `prev` becomes `next`: on a profile switch,
/// the new profile's values are loaded. Either way the active profile's
/// values are recorded in `saved`.
pub fn apply(prev: &Settings, next: Settings, saved: &mut Saved) -> Settings {
    let mut v = serde_json::to_value(&next).unwrap();
    let snapshot = |v: &Value| -> BTreeMap<String, Value> {
        KEYS.iter()
            .map(|k| (k.to_string(), get(v, k).clone()))
            .collect()
    };
    let (old, new) = (prev.profiles.active, next.profiles.active);
    if old != new {
        saved.insert(name(old), snapshot(&v));
        let theirs = saved.get(&name(new)).cloned();
        for (k, value) in preset(new) {
            let value = theirs
                .as_ref()
                .and_then(|t| t.get(k).cloned())
                .unwrap_or(value);
            set(&mut v, k, value);
        }
    }
    saved.insert(name(new), snapshot(&v));
    // A saved value that no longer fits (the setting changed type) is
    // dropped in favor of what was there.
    serde_json::from_value(v).unwrap_or(next)
}

/// The saved values on disk, next to settings.json.
pub struct Store {
    path: PathBuf,
    pub saved: Mutex<Saved>,
}

impl Store {
    pub fn load(settings_path: &Path) -> Self {
        let path = settings_path.with_file_name("profiles.json");
        let saved = fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self {
            path,
            saved: Mutex::new(saved),
        }
    }

    pub fn save(&self) {
        let text = serde_json::to_string_pretty(&*self.saved.lock().unwrap()).unwrap();
        if let Err(e) = fs::write(&self.path, text) {
            log::warn!("couldn't save profiles: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::schema::{Detail, ReadAloud};

    fn switch(s: &Settings, to: BuiltinProfile, saved: &mut Saved) -> Settings {
        let mut next = s.clone();
        next.profiles.active = to;
        apply(s, next, saved)
    }

    #[test]
    fn every_key_is_a_real_setting() {
        let d = serde_json::to_value(Settings::default()).unwrap();
        for k in KEYS {
            assert!(!get(&d, k).is_null(), "{k}");
        }
        for p in [BuiltinProfile::Expert, BuiltinProfile::Quiet] {
            let keys: Vec<_> = preset(p).into_iter().map(|(k, _)| k).collect();
            assert_eq!(keys, KEYS);
        }
    }

    #[test]
    fn switching_loads_presets_and_remembers_changes_per_profile() {
        let mut saved = Saved::new();
        let beginner = Settings::default();
        let expert = switch(&beginner, BuiltinProfile::Expert, &mut saved);
        assert_eq!(expert.answer_style.detail, Detail::Brief);
        assert!(!expert.voice_output.voice_guidance);
        // A change made in Expert stays with Expert.
        let mut tweaked = expert.clone();
        tweaked.answer_style.detail = Detail::Detailed;
        tweaked.buddy.size = 60; // not a profile setting
        let tweaked = apply(&expert, tweaked, &mut saved);
        let back = switch(&tweaked, BuiltinProfile::Beginner, &mut saved);
        assert_eq!(back.answer_style.detail, Detail::Normal);
        assert!(back.voice_output.voice_guidance);
        assert_eq!(back.buddy.size, 60);
        let again = switch(&back, BuiltinProfile::Expert, &mut saved);
        assert_eq!(again.answer_style.detail, Detail::Detailed);
        let quiet = switch(&again, BuiltinProfile::Quiet, &mut saved);
        assert!(quiet.guidance.reduce_motion && !quiet.agents.notifications);
        assert_eq!(quiet.voice_output.read_aloud, ReadAloud::StepsOnly);
    }

    #[test]
    fn beginner_changes_made_before_switching_are_kept() {
        let mut saved = Saved::new();
        let mut s = Settings::default();
        s.voice_output.voice_guidance = false;
        let expert = switch(&s, BuiltinProfile::Expert, &mut saved);
        let back = switch(&expert, BuiltinProfile::Beginner, &mut saved);
        assert!(!back.voice_output.voice_guidance);
    }
}
