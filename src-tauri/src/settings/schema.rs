//! The one typed settings schema.
//!
//! Every struct here is exported to TypeScript by `ts-rs` (run `cargo test export_bindings`),
//! so the frontend never re-declares a setting's shape. Defaults live in the `Default` impls
//! and are also exported as `src/bindings/defaultSettings.json`.
//!
//! Rules for adding a setting:
//! - add the field with `#[serde(default)]` coverage (the struct-level attribute handles it),
//! - set its default below,
//! - add range or format checks to `validate` in `validate.rs` if it needs any,
//! - add one registry entry in `src/settings/registry.tsx` for label, help, and control.
//!
//! Secrets (API keys, OAuth tokens) never live in this struct. They go to the OS keychain,
//! which is why export and import can serialize `Settings` as-is.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Settings {
    pub version: u32,
    pub general: GeneralSettings,
    pub buddy: BuddySettings,
    pub hotkeys: HotkeySettings,
    pub voice_output: VoiceOutputSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            general: GeneralSettings::default(),
            buddy: BuddySettings::default(),
            hotkeys: HotkeySettings::default(),
            voice_output: VoiceOutputSettings::default(),
        }
    }
}

/// Section ids, used by "reset to defaults" and by the settings search index.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SectionId {
    General,
    Buddy,
    Hotkeys,
    VoiceOutput,
}

// ---------------------------------------------------------------- General

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct GeneralSettings {
    pub launch_at_login: bool,
    pub start_minimized: bool,
    pub theme: ThemePreference,
    /// BCP 47 tag for the interface, for example "en" or "de".
    pub interface_language: String,
    /// "auto" answers in the language the user spoke, otherwise a BCP 47 tag.
    pub response_language: String,
    pub check_for_updates: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            start_minimized: true,
            theme: ThemePreference::System,
            interface_language: "en".into(),
            response_language: "auto".into(),
            check_for_updates: true,
        }
    }
}

// ---------------------------------------------------------------- Buddy

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum BuddyStyle {
    /// Round coral character with a face. The default.
    Pip,
    /// A quiet ring with an orbiting dot.
    Orbit,
    /// A four-point spark.
    Spark,
    /// Soft ink square with eyes.
    Pebble,
    /// The user's own SVG or PNG, see `custom_image`.
    Custom,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct BuddySettings {
    pub enabled: bool,
    pub style: BuddyStyle,
    /// File name of the uploaded image inside the app data `buddy/` folder.
    pub custom_image: Option<String>,
    /// Size in logical pixels (24 to 96).
    pub size: u32,
    /// 0.2 to 1.0.
    pub opacity: f32,
    /// Offset from the cursor hotspot in logical pixels (-120 to 120).
    pub offset_x: i32,
    pub offset_y: i32,
    /// 0 snaps to the cursor every frame, 1 is a slow glide.
    pub smoothness: f32,
    pub show_state_animations: bool,
    pub show_agent_badge: bool,
    pub hide_in_fullscreen: bool,
    pub hide_when_idle: bool,
    /// Seconds of mouse inactivity before hiding (3 to 600).
    pub idle_seconds: u32,
}

impl Default for BuddySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            style: BuddyStyle::Pip,
            custom_image: None,
            size: 36,
            opacity: 1.0,
            offset_x: 22,
            offset_y: 20,
            smoothness: 0.2,
            show_state_animations: true,
            show_agent_badge: true,
            hide_in_fullscreen: true,
            hide_when_idle: false,
            idle_seconds: 20,
        }
    }
}

// ---------------------------------------------------------------- Hotkeys

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum VoiceHotkeyMode {
    /// Hold the hotkey while speaking.
    PushToTalk,
    /// Press once to start, again to stop.
    Toggle,
}

/// Accelerator strings in Tauri's format, for example "Alt+Shift+H".
/// An empty string means the action has no hotkey.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct HotkeySettings {
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

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            voice_ask: "Alt+Shift+H".into(),
            voice_mode: VoiceHotkeyMode::PushToTalk,
            text_ask: "Alt+Shift+T".into(),
            circle_to_explain: "Alt+Shift+E".into(),
            clear_annotations: "Alt+Shift+X".into(),
            pause_capture: "Alt+Shift+P".into(),
            open_agent_panel: "Alt+Shift+A".into(),
            open_approval_inbox: "Alt+Shift+I".into(),
            pause_all_agents: "Alt+Shift+Z".into(),
            open_settings: "Alt+Shift+Comma".into(),
        }
    }
}

/// Stable ids for hotkey actions, shared with the frontend.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HotkeyAction {
    VoiceAsk,
    TextAsk,
    CircleToExplain,
    ClearAnnotations,
    PauseCapture,
    OpenAgentPanel,
    OpenApprovalInbox,
    PauseAllAgents,
    OpenSettings,
}

impl HotkeyAction {
    pub const ALL: [HotkeyAction; 9] = [
        HotkeyAction::VoiceAsk,
        HotkeyAction::TextAsk,
        HotkeyAction::CircleToExplain,
        HotkeyAction::ClearAnnotations,
        HotkeyAction::PauseCapture,
        HotkeyAction::OpenAgentPanel,
        HotkeyAction::OpenApprovalInbox,
        HotkeyAction::PauseAllAgents,
        HotkeyAction::OpenSettings,
    ];

    /// The camelCase field name, which is also the settings path under `hotkeys.`.
    pub fn key(self) -> &'static str {
        match self {
            HotkeyAction::VoiceAsk => "voiceAsk",
            HotkeyAction::TextAsk => "textAsk",
            HotkeyAction::CircleToExplain => "circleToExplain",
            HotkeyAction::ClearAnnotations => "clearAnnotations",
            HotkeyAction::PauseCapture => "pauseCapture",
            HotkeyAction::OpenAgentPanel => "openAgentPanel",
            HotkeyAction::OpenApprovalInbox => "openApprovalInbox",
            HotkeyAction::PauseAllAgents => "pauseAllAgents",
            HotkeyAction::OpenSettings => "openSettings",
        }
    }
}

impl HotkeySettings {
    pub fn get(&self, action: HotkeyAction) -> &str {
        match action {
            HotkeyAction::VoiceAsk => &self.voice_ask,
            HotkeyAction::TextAsk => &self.text_ask,
            HotkeyAction::CircleToExplain => &self.circle_to_explain,
            HotkeyAction::ClearAnnotations => &self.clear_annotations,
            HotkeyAction::PauseCapture => &self.pause_capture,
            HotkeyAction::OpenAgentPanel => &self.open_agent_panel,
            HotkeyAction::OpenApprovalInbox => &self.open_approval_inbox,
            HotkeyAction::PauseAllAgents => &self.pause_all_agents,
            HotkeyAction::OpenSettings => &self.open_settings,
        }
    }
}

// ---------------------------------------------------------------- Voice output
// Only the on/off switch exists so far (the tray toggles it). Phase 3 fills in the rest.

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct VoiceOutputSettings {
    pub guidance_enabled: bool,
}

impl Default for VoiceOutputSettings {
    fn default() -> Self {
        Self { guidance_enabled: true }
    }
}
