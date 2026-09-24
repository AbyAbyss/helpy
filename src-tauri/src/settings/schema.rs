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
    pub ai: Ai,
    pub answer_style: AnswerStyle,
    pub limits: Limits,
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
    "ai",
    "answerStyle",
    "limits",
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

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ProviderKind {
    Anthropic,
    OpenAi,
    Gemini,
    Ollama,
    LmStudio,
    LlamaCpp,
    /// Any server that speaks the OpenAI chat completions API.
    OpenAiCompatible,
}

impl ProviderKind {
    /// Cloud APIs that can't be used without a key. Everything else may
    /// run without one, so an unreadable keychain mustn't block it.
    pub fn requires_key(self) -> bool {
        matches!(self, Self::Anthropic | Self::OpenAi | Self::Gemini)
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct ModelConfig {
    pub id: String,
    /// Can read images. Detected when the model is added; the user can override it.
    pub vision: bool,
    /// Calls tools reliably. When false, Helpy doesn't offer it tools.
    pub tools: bool,
    /// Price in US dollars per million tokens. None means unknown.
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            vision: true,
            tools: true,
            input_price: None,
            output_price: None,
        }
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderConfig {
    /// Stable id; also names the provider's API key in the OS keychain.
    pub id: String,
    pub kind: ProviderKind,
    pub name: String,
    pub base_url: String,
    pub models: Vec<ModelConfig>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: ProviderKind::OpenAiCompatible,
            name: String::new(),
            base_url: String::new(),
            models: Vec::new(),
        }
    }
}

/// A specific model on a specific provider.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Eq, Hash, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct ModelRef {
    pub provider_id: String,
    pub model: String,
}

/// Which model each feature uses. None means "not set up yet".
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Routing {
    /// Voice and text questions.
    pub ask: Option<ModelRef>,
    pub visual_guidance: Option<ModelRef>,
    pub circle_to_explain: Option<ModelRef>,
    pub agent_planning: Option<ModelRef>,
    pub agent_orchestrator: Option<ModelRef>,
    pub agent_worker: Option<ModelRef>,
    /// Used for a request that needs the screen when its model can't read images.
    pub vision_fallback: Option<ModelRef>,
}

impl Routing {
    pub fn entries(&self) -> [(&'static str, &Option<ModelRef>); 7] {
        [
            ("ask", &self.ask),
            ("visualGuidance", &self.visual_guidance),
            ("circleToExplain", &self.circle_to_explain),
            ("agentPlanning", &self.agent_planning),
            ("agentOrchestrator", &self.agent_orchestrator),
            ("agentWorker", &self.agent_worker),
            ("visionFallback", &self.vision_fallback),
        ]
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Ai {
    pub providers: Vec<ProviderConfig>,
    pub routing: Routing,
    /// Tried in order, one attempt each, when a feature's model fails.
    pub fallback_chain: Vec<ModelRef>,
    pub temperature: f64,
    pub max_response_tokens: u32,
    /// Give up on a request after this many seconds without any data.
    pub timeout_secs: u32,
    /// Added to every request, e.g. "I use Windows 11 and Outlook desktop".
    pub custom_instructions: String,
}

impl Default for Ai {
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            routing: Routing::default(),
            fallback_chain: Vec::new(),
            temperature: 0.7,
            max_response_tokens: 16000,
            timeout_secs: 60,
            custom_instructions: String::new(),
        }
    }
}

impl Ai {
    pub fn provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn model(&self, r: &ModelRef) -> Option<(&ProviderConfig, &ModelConfig)> {
        let p = self.provider(&r.provider_id)?;
        Some((p, p.models.iter().find(|m| m.id == r.model)?))
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Detail {
    Brief,
    #[default]
    Normal,
    Detailed,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Tone {
    #[default]
    Casual,
    Formal,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ScreenAccess {
    /// Every question includes a screenshot.
    Always,
    /// The AI may ask; the user confirms each time.
    Ask,
    /// The AI decides.
    #[default]
    WhenNeeded,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AnswerStyle {
    pub detail: Detail,
    pub tone: Tone,
    pub screen_access: ScreenAccess,
}

/// Retry and spending limits, enforced in Rust for every model call.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Limits {
    pub max_retries: u32,
    pub backoff_base_ms: u32,
    pub backoff_max_ms: u32,
    /// 0 turns the daily token limit off.
    #[ts(type = "number")]
    pub daily_token_budget: u64,
    /// US dollars per day. None turns the daily cost limit off.
    pub daily_cost_budget: Option<f64>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff_base_ms: 2000,
            backoff_max_ms: 30000,
            daily_token_budget: 2_000_000,
            daily_cost_budget: None,
        }
    }
}
