//! The single settings schema. TypeScript types in `src/bindings/` are generated
//! from these structs by `ts-rs` (run `npm run bindings`).
//!
//! Every struct uses `#[serde(default)]`, so a settings file written by an older
//! version (missing fields) or a newer one (extra fields) still loads.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Settings {
    pub general: General,
    pub profiles: Profiles,
    pub buddy: Buddy,
    pub hotkeys: Hotkeys,
    pub voice_output: VoiceOutput,
    pub privacy: Privacy,
}

/// Top-level keys that the settings page treats as sections. Used for
/// "reset to defaults" per section.
pub const SECTIONS: &[&str] = &[
    "general",
    "profiles",
    "buddy",
    "hotkeys",
    "voiceOutput",
    "privacy",
];

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct General {
    pub launch_at_login: bool,
    pub start_minimized: bool,
    pub theme: Theme,
    /// BCP 47 tag. Only "en" ships in v1.
    pub interface_language: String,
    /// "auto" answers in the language the user spoke, otherwise a BCP 47 tag.
    pub response_language: String,
    pub check_for_updates: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            start_minimized: true,
            theme: Theme::System,
            interface_language: "en".into(),
            response_language: "auto".into(),
            check_for_updates: true,
        }
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum BuiltinProfile {
    #[default]
    Beginner,
    Expert,
    Quiet,
}

/// Profile contents (what each one changes) arrive in Phase 9. Phase 1 stores
/// which one is active so the tray can switch it.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Profiles {
    pub active: BuiltinProfile,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum BuddyStyle {
    #[default]
    Pip,
    Spark,
    Dot,
    Custom,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Buddy {
    pub enabled: bool,
    pub style: BuddyStyle,
    /// Rendered size in logical pixels.
    pub size: u32,
    /// 0.2 to 1.0.
    pub opacity: f64,
    pub offset_x: i32,
    pub offset_y: i32,
    /// 0 snaps to the cursor every frame; higher values trail more.
    pub smoothness: f64,
    pub show_state_animations: bool,
    pub show_agent_badge: bool,
    pub hide_in_fullscreen: bool,
    pub hide_when_idle: bool,
    pub idle_seconds: u32,
}

impl Default for Buddy {
    fn default() -> Self {
        Self {
            enabled: true,
            style: BuddyStyle::Pip,
            size: 36,
            opacity: 1.0,
            offset_x: 18,
            offset_y: 20,
            smoothness: 0.35,
            show_state_animations: true,
            show_agent_badge: true,
            hide_in_fullscreen: true,
            hide_when_idle: false,
            idle_seconds: 10,
        }
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum VoiceHotkeyMode {
    #[default]
    PushToTalk,
    Toggle,
}

/// Accelerator strings in the format `tauri-plugin-global-shortcut` parses,
/// for example "Alt+Shift+Space". An empty string means unbound.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Hotkeys {
    pub voice_ask: String,
    pub voice_mode: VoiceHotkeyMode,
    pub text_ask: String,
    pub circle_to_explain: String,
    pub clear_annotations: String,
    pub pause_capture: String,
    pub open_agent_panel: String,
    pub open_approval_inbox: String,
    pub pause_all_agents: String,
    pub open_settings: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            voice_ask: "Alt+Shift+Space".into(),
            voice_mode: VoiceHotkeyMode::PushToTalk,
            text_ask: "Alt+Shift+T".into(),
            circle_to_explain: "Alt+Shift+C".into(),
            clear_annotations: "Alt+Shift+X".into(),
            pause_capture: "Alt+Shift+P".into(),
            open_agent_panel: "Alt+Shift+A".into(),
            open_approval_inbox: "Alt+Shift+I".into(),
            pause_all_agents: "Alt+Shift+Z".into(),
            open_settings: "Alt+Shift+S".into(),
        }
    }
}

impl Hotkeys {
    /// (camelCase key, human label, accelerator) for every bindable action.
    pub fn actions(&self) -> [(&'static str, &'static str, &str); 9] {
        [
            ("voiceAsk", "Voice ask", &self.voice_ask),
            ("textAsk", "Text ask", &self.text_ask),
            (
                "circleToExplain",
                "Circle to explain",
                &self.circle_to_explain,
            ),
            (
                "clearAnnotations",
                "Clear annotations",
                &self.clear_annotations,
            ),
            ("pauseCapture", "Pause screen capture", &self.pause_capture),
            ("openAgentPanel", "Open agent panel", &self.open_agent_panel),
            (
                "openApprovalInbox",
                "Open approval inbox",
                &self.open_approval_inbox,
            ),
            ("pauseAllAgents", "Pause all agents", &self.pause_all_agents),
            ("openSettings", "Open settings", &self.open_settings),
        ]
    }
}

/// Only the on/off switch exists before Phase 3.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct VoiceOutput {
    pub voice_guidance: bool,
}

impl Default for VoiceOutput {
    fn default() -> Self {
        Self {
            voice_guidance: true,
        }
    }
}

/// Only the capture pause switch exists before Phase 9.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Privacy {
    pub capture_paused: bool,
}
