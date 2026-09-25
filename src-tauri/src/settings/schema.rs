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
    pub voice_input: VoiceInput,
    pub voice_output: VoiceOutput,
    pub privacy: Privacy,
    pub ai: Ai,
    pub answer_style: AnswerStyle,
    pub guidance: Guidance,
    pub circle: Circle,
    pub agents: Agents,
    pub connectors: Connectors,
    pub limits: Limits,
}

/// Top-level keys that the settings page treats as sections. Used for
/// "reset to defaults" per section.
pub const SECTIONS: &[&str] = &[
    "general",
    "profiles",
    "buddy",
    "hotkeys",
    "voiceInput",
    "voiceOutput",
    "privacy",
    "ai",
    "answerStyle",
    "guidance",
    "circle",
    "agents",
    "connectors",
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

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SttEngine {
    /// whisper.cpp on this computer.
    #[default]
    Whisper,
    OpenAi,
    Deepgram,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct VoiceInput {
    pub engine: SttEngine,
    /// Name of the local Whisper model, e.g. "base" (file ggml-base.bin).
    pub whisper_model: String,
    /// An AI provider (OpenAI or OpenAI-compatible) whose key is used.
    pub openai_provider_id: Option<String>,
    pub openai_model: String,
    pub deepgram_model: String,
    /// Input device name. None uses the system default.
    pub microphone: Option<String>,
    /// "auto" or a language code.
    pub language: String,
    /// Stop listening after this much silence. 0 turns auto-stop off.
    pub silence_seconds: f64,
    pub noise_suppression: bool,
    pub wake_word: bool,
    pub wake_phrase: String,
}

impl Default for VoiceInput {
    fn default() -> Self {
        Self {
            engine: SttEngine::Whisper,
            whisper_model: "base".into(),
            openai_provider_id: None,
            openai_model: "whisper-1".into(),
            deepgram_model: "nova-3".into(),
            microphone: None,
            language: "auto".into(),
            silence_seconds: 1.5,
            noise_suppression: true,
            wake_word: false,
            wake_phrase: "hey helpy".into(),
        }
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TtsEngine {
    /// The voices built into the operating system.
    #[default]
    System,
    Piper,
    OpenAi,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ReadAloud {
    #[default]
    FullAnswers,
    StepsOnly,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct VoiceOutput {
    pub voice_guidance: bool,
    pub engine: TtsEngine,
    /// OS voice id. None uses the system default voice.
    pub system_voice: Option<String>,
    /// Piper voice key, e.g. "en_US-lessac-medium".
    pub piper_voice: String,
    pub openai_provider_id: Option<String>,
    pub openai_model: String,
    pub openai_voice: String,
    /// 1.0 is normal speed.
    pub speed: f64,
    /// 0 to 1.
    pub volume: f64,
    pub read_aloud: ReadAloud,
    pub announce_agents: bool,
}

impl Default for VoiceOutput {
    fn default() -> Self {
        Self {
            voice_guidance: true,
            engine: TtsEngine::System,
            system_voice: None,
            piper_voice: "en_US-lessac-medium".into(),
            openai_provider_id: None,
            openai_model: "tts-1".into(),
            openai_voice: "alloy".into(),
            speed: 1.0,
            volume: 1.0,
            read_aloud: ReadAloud::FullAnswers,
            announce_agents: true,
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

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LabelStyle {
    /// Filled with the highlight colour.
    #[default]
    Accent,
    Dark,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ArrowStyle {
    #[default]
    Curved,
    Straight,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Advance {
    /// A click on or near the target moves to the next step.
    #[default]
    OnClick,
    /// Only the Next button does.
    NextButton,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CardPosition {
    #[default]
    NearTarget,
    Top,
    Bottom,
}

/// How highlights, pointers and walkthroughs look and behave.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Guidance {
    /// "#rrggbb".
    pub highlight_color: String,
    /// Outline width in logical pixels.
    pub highlight_thickness: u32,
    pub glow: bool,
    /// How dark everything outside a highlight gets, 0 (off) to 0.7.
    pub dim: f64,
    pub label_style: LabelStyle,
    pub arrow_style: ArrowStyle,
    pub advance: Advance,
    /// Hide the marks after this many seconds; the step card stays. 0 keeps
    /// them until the step is done.
    pub annotation_seconds: u32,
    pub card_position: CardPosition,
    /// 1.0 is normal; higher is faster.
    pub animation_speed: f64,
    pub reduce_motion: bool,
    /// Steps per walkthrough before Helpy stops.
    pub max_steps: u32,
    /// Shows the model's raw coordinates next to each mark.
    pub show_coordinates: bool,
}

impl Default for Guidance {
    fn default() -> Self {
        Self {
            highlight_color: "#e5484d".into(),
            highlight_thickness: 3,
            glow: true,
            dim: 0.25,
            label_style: LabelStyle::Accent,
            arrow_style: ArrowStyle::Curved,
            advance: Advance::OnClick,
            annotation_seconds: 0,
            card_position: CardPosition::NearTarget,
            animation_speed: 1.0,
            reduce_motion: false,
            max_steps: 12,
            show_coordinates: false,
        }
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SelectionShape {
    #[default]
    Rectangle,
    Freehand,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CircleAction {
    #[default]
    Explain,
    /// Copy the text in the selection (OCR by the vision model).
    CopyText,
    Translate,
    Summarize,
    /// Show the actions and let the user pick.
    Menu,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LabelDetail {
    /// Just the part's name.
    #[default]
    Names,
    /// The name and a short note.
    NamesAndNotes,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Circle {
    pub default_shape: SelectionShape,
    pub default_action: CircleAction,
    pub label_detail: LabelDetail,
    /// Language code that Translate translates into.
    pub translate_to: String,
}

impl Default for Circle {
    fn default() -> Self {
        Self {
            default_shape: SelectionShape::Rectangle,
            default_action: CircleAction::Explain,
            label_detail: LabelDetail::Names,
            translate_to: "en".into(),
        }
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DefaultRunMode {
    /// The planner picks from the request.
    #[default]
    Auto,
    Parallel,
    Sequential,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConfirmPlans {
    #[default]
    Always,
    /// Plans that only read and search start right away.
    SideEffectsOnly,
}

/// What happens when an agent wants to do something of this kind.
#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Rule {
    Allow,
    #[default]
    Ask,
    Never,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Approvals {
    /// Writing, moving and renaming files in approved folders (backed up, undoable).
    pub file_changes: Rule,
    pub file_deletes: Rule,
    /// Reminders and calendar events. (Shell commands follow the shell policy.)
    pub reminders: Rule,
}

impl Default for Approvals {
    fn default() -> Self {
        Self {
            file_changes: Rule::Allow,
            file_deletes: Rule::Ask,
            reminders: Rule::Ask,
        }
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ShellPolicy {
    Never,
    /// Every command waits for approval.
    #[default]
    Ask,
    /// Only commands starting with an entry of the allowlist run, without asking.
    Allowlist,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SearchEngine {
    /// Brave when its key is set, otherwise SearXNG when its address is set,
    /// otherwise DuckDuckGo.
    #[default]
    Auto,
    DuckDuckGo,
    Brave,
    Searxng,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AgentTools {
    pub web_search: bool,
    pub fetch: bool,
    pub files: bool,
    pub shell: bool,
    pub reminders: bool,
}

impl Default for AgentTools {
    fn default() -> Self {
        Self {
            web_search: true,
            fetch: true,
            files: true,
            shell: true,
            reminders: true,
        }
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OnFailure {
    #[default]
    Stop,
    /// Pause and ask whether to retry, skip the step, or cancel.
    Ask,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DockSide {
    #[default]
    Right,
    Left,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Agents {
    // Running and approvals
    pub max_running: u32,
    pub default_mode: DefaultRunMode,
    pub confirm_plans: ConfirmPlans,
    pub approvals: Approvals,

    // Tools and access
    pub tools: AgentTools,
    /// Folders agents may read and change, as absolute paths.
    pub approved_folders: Vec<String>,
    /// Where builder agents make new projects. Empty uses "Helpy Projects"
    /// in the home folder.
    pub projects_folder: String,
    /// Days to keep backups of files agents changed.
    pub backup_days: u32,
    pub shell_policy: ShellPolicy,
    pub shell_allowlist: Vec<String>,
    pub search_engine: SearchEngine,
    /// A SearXNG instance, e.g. "http://localhost:8888".
    pub searxng_url: String,

    // Limits
    pub time_limit_minutes: u32,
    /// Per agent. 0 turns it off.
    #[ts(type = "number")]
    pub agent_token_budget: u64,
    pub agent_cost_budget: Option<f64>,
    /// Per batch of agents started together. 0 turns it off.
    #[ts(type = "number")]
    pub batch_token_budget: u64,
    pub batch_cost_budget: Option<f64>,
    pub on_failure: OnFailure,
    pub max_steps: u32,
    pub max_tool_calls: u32,
    /// The same tool with the same arguments this many times means stuck.
    pub repeat_threshold: u32,
    /// This many steps without anything new means stuck.
    pub no_progress_steps: u32,
    /// Older work is summarized once an agent's conversation passes this.
    pub context_tokens: u32,

    // Dock and notifications
    pub dock: bool,
    pub dock_side: DockSide,
    /// Read each new status line aloud.
    pub speak_status: bool,
    /// Finished agents leave the dock after this many seconds. 0 keeps them.
    pub done_seconds: u32,
    pub notifications: bool,
    /// Days to keep finished agents in the panel.
    pub history_days: u32,

    /// The user's own templates (built-in ones live in the code).
    pub templates: Vec<Template>,
}

impl Default for Agents {
    fn default() -> Self {
        Self {
            max_running: 3,
            default_mode: DefaultRunMode::Auto,
            confirm_plans: ConfirmPlans::Always,
            approvals: Approvals::default(),
            tools: AgentTools::default(),
            approved_folders: Vec::new(),
            projects_folder: String::new(),
            backup_days: 14,
            shell_policy: ShellPolicy::Ask,
            shell_allowlist: Vec::new(),
            search_engine: SearchEngine::Auto,
            searxng_url: String::new(),
            time_limit_minutes: 30,
            agent_token_budget: 400_000,
            agent_cost_budget: None,
            batch_token_budget: 1_000_000,
            batch_cost_budget: None,
            on_failure: OnFailure::Stop,
            max_steps: 25,
            max_tool_calls: 50,
            repeat_threshold: 3,
            no_progress_steps: 8,
            context_tokens: 60_000,
            dock: true,
            dock_side: DockSide::Right,
            speak_status: false,
            done_seconds: 60,
            notifications: true,
            history_days: 30,
            templates: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ParamKind {
    #[default]
    Text,
    LongText,
    Folder,
    Number,
    Choice,
}

/// Something the user fills in when starting a template, used in its
/// goals, names and folders as {key}.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct TemplateParam {
    pub key: String,
    pub label: String,
    pub kind: ParamKind,
    pub default: String,
    /// For a choice.
    pub options: Vec<String>,
    pub required: bool,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct TemplateAgent {
    pub name: String,
    pub goal: String,
    pub tools: Vec<String>,
    pub keep_open: bool,
    /// Names of earlier agents of the template it waits for.
    pub after: Vec<String>,
}

/// A task to start again and again, with blanks to fill in.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
    pub params: Vec<TemplateParam>,
    pub agents: Vec<TemplateAgent>,
    /// Folders it works in; approved when the plan starts.
    pub folders: Vec<String>,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Permission {
    #[default]
    ReadOnly,
    ReadWrite,
}

/// A built-in connector's settings. Whether it's connected, and as whom,
/// isn't a setting; tokens live in the keychain.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectorConfig {
    pub permission: Permission,
    /// Per-action overrides of the approval rule, by action name.
    pub rules: std::collections::BTreeMap<String, Rule>,
}

/// The user's own OAuth app for a provider (google, microsoft, notion,
/// slack, github). The client secret, where one is needed, is in the keychain.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct OAuthApp {
    pub client_id: String,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum McpTransport {
    /// A local program Helpy starts, speaking MCP over stdin and stdout.
    #[default]
    Stdio,
    /// A server at a URL, speaking Streamable HTTP.
    Http,
}

/// An environment variable or HTTP header. Secret values are kept in the
/// keychain and are empty here.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    pub secret: bool,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct McpServer {
    /// Stable id; names the server's secrets in the keychain.
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub transport: McpTransport,
    // Stdio
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: Vec<KeyValue>,
    // HTTP
    pub url: String,
    pub headers: Vec<KeyValue>,
    /// Sign in with OAuth (discovered from the server, as the MCP spec says).
    pub oauth: bool,
    /// A pre-registered OAuth client, for servers without dynamic registration.
    pub oauth_client_id: String,
    pub oauth_scopes: String,
    /// Read-only offers only tools the server marks as read-only.
    pub permission: Permission,
    /// Per-tool overrides of the approval rule.
    pub rules: std::collections::BTreeMap<String, Rule>,
    /// The catalog entry it came from, if any.
    pub preset: String,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Connectors {
    /// OAuth apps by provider.
    pub apps: std::collections::BTreeMap<String, OAuthApp>,
    /// Built-in connectors by id. Missing means the defaults.
    pub builtin: std::collections::BTreeMap<String, ConnectorConfig>,
    pub mcp: Vec<McpServer>,
}

impl Connectors {
    pub fn config(&self, id: &str) -> ConnectorConfig {
        self.builtin.get(id).cloned().unwrap_or_default()
    }
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
