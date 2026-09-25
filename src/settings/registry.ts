// Everything the settings page knows about each setting: where it lives,
// what it's called, and how to edit it. Rendering and search both read this.
// Types and validation come from the Rust schema (src-tauri/src/settings).

import type { Settings } from "../bindings/Settings";
import type { SettingPath } from "../lib/ipc";

export type SectionId = "general" | "buddy" | "hotkeys" | "ai" | "answerStyle" | "guidance" | "voiceInput" | "voiceOutput";

export const SECTIONS: { id: SectionId; title: string; blurb: string }[] = [
  { id: "general", title: "General", blurb: "Startup, appearance and language." },
  { id: "buddy", title: "Cursor buddy", blurb: "The small character that rides along with your mouse." },
  { id: "hotkeys", title: "Hotkeys", blurb: "Shortcuts that work from any app. Click one to change it." },
  { id: "ai", title: "AI providers", blurb: "The models Helpy talks to, what each one is used for, and how much it may spend." },
  { id: "answerStyle", title: "Answer style", blurb: "How Helpy answers, and when it may look at your screen." },
  { id: "guidance", title: "Visual guidance", blurb: "How Helpy points things out on your screen, and how walkthroughs move from step to step." },
  { id: "voiceInput", title: "Voice input", blurb: "How Helpy hears you: the microphone, the speech engine, and when it stops listening." },
  { id: "voiceOutput", title: "Voice output", blurb: "Whether Helpy reads answers aloud, and in which voice." },
];

/** `requires` hides an option on computers that can't use it. */
type Option = { value: string; label: string; requires?: "piper" };

export type Control =
  | { kind: "toggle" }
  | { kind: "segmented"; options: Option[] }
  | { kind: "select"; options: Option[] }
  | { kind: "slider"; min: number; max: number; step: number; format: (v: number) => string; ends?: [string, string] }
  | { kind: "number"; min: number; max: number; unit: string }
  | { kind: "hotkey" }
  | { kind: "textarea"; placeholder: string; max: number }
  /** A dollar amount that can be left empty to turn it off. */
  | { kind: "money"; emptyLabel: string }
  | { kind: "text"; placeholder: string }
  | { kind: "color"; swatches: string[] }
  | { kind: "buddyStyle" }
  | { kind: "whisperModels" }
  | { kind: "micPicker" }
  /** An AI provider whose key a speech service borrows (OpenAI-style only). */
  | { kind: "providerSelect" }
  | { kind: "deepgram" }
  | { kind: "systemVoice" }
  | { kind: "piperVoice" }
  | { kind: "providers" }
  | { kind: "routing" }
  | { kind: "fallbackChain" };

export type Field = {
  path: SettingPath;
  section: SectionId;
  group: string;
  label: string;
  help?: string;
  /** Extra words people might search for. */
  keywords?: string;
  control: Control;
  /** Hide the row when it has no effect. */
  when?: (s: Settings) => boolean;
};

const px = (v: number) => `${v} px`;
const pct = (v: number) => `${Math.round(v * 100)}%`;

const RESPONSE_LANGUAGES: Option[] = [
  { value: "auto", label: "Match what I say" },
  { value: "en", label: "English" },
  { value: "de", label: "Deutsch" },
  { value: "es", label: "Español" },
  { value: "fr", label: "Français" },
  { value: "it", label: "Italiano" },
  { value: "nl", label: "Nederlands" },
  { value: "pl", label: "Polski" },
  { value: "pt-BR", label: "Português (Brasil)" },
  { value: "hi", label: "हिन्दी" },
  { value: "ja", label: "日本語" },
  { value: "ko", label: "한국어" },
  { value: "zh-Hans", label: "简体中文" },
];

export const FIELDS: Field[] = [
  // General
  {
    path: "general.launchAtLogin", section: "general", group: "Startup", label: "Launch at login",
    help: "Scheduled agents only run while Helpy is running, so turn this on if you use them.",
    keywords: "autostart boot startup login", control: { kind: "toggle" },
  },
  {
    path: "general.startMinimized", section: "general", group: "Startup", label: "Start in the tray",
    help: "Helpy starts quietly in the tray instead of opening this window.",
    keywords: "minimized hidden", control: { kind: "toggle" },
  },
  {
    path: "general.checkForUpdates", section: "general", group: "Startup", label: "Check for updates automatically",
    keywords: "update version", control: { kind: "toggle" },
  },
  {
    path: "general.theme", section: "general", group: "Appearance", label: "Theme",
    help: "System follows your OS light or dark mode.", keywords: "dark mode light mode colors appearance",
    control: {
      kind: "segmented",
      options: [
        { value: "system", label: "System" },
        { value: "light", label: "Light" },
        { value: "dark", label: "Dark" },
      ],
    },
  },
  {
    path: "general.interfaceLanguage", section: "general", group: "Language", label: "Interface language",
    help: "More languages are on the way.", keywords: "locale translation",
    control: { kind: "select", options: [{ value: "en", label: "English" }] },
  },
  {
    path: "general.responseLanguage", section: "general", group: "Language", label: "Answer language",
    help: "The language Helpy answers in, out loud and on screen.",
    keywords: "ai reply response locale", control: { kind: "select", options: RESPONSE_LANGUAGES },
  },

  // Cursor buddy
  {
    path: "buddy.enabled", section: "buddy", group: "Buddy", label: "Show the cursor buddy",
    help: "Also in the tray menu. The buddy never blocks clicks.", keywords: "toggle on off character",
    control: { kind: "toggle" },
  },
  {
    path: "buddy.style", section: "buddy", group: "Buddy", label: "Style",
    help: "Pick a built-in buddy or upload an SVG or PNG up to 1 MB.", keywords: "character image upload custom svg png",
    control: { kind: "buddyStyle" },
  },
  {
    path: "buddy.size", section: "buddy", group: "Look", label: "Size", keywords: "scale big small",
    control: { kind: "slider", min: 16, max: 128, step: 2, format: px },
  },
  {
    path: "buddy.opacity", section: "buddy", group: "Look", label: "Opacity", keywords: "transparency see-through",
    control: { kind: "slider", min: 0.2, max: 1, step: 0.05, format: pct },
  },
  {
    path: "buddy.offsetX", section: "buddy", group: "Position", label: "Horizontal offset",
    help: "Distance from the pointer tip. Negative moves it left.", keywords: "x position distance",
    control: { kind: "number", min: -200, max: 200, unit: "px" },
  },
  {
    path: "buddy.offsetY", section: "buddy", group: "Position", label: "Vertical offset",
    help: "Negative moves it above the pointer.", keywords: "y position distance",
    control: { kind: "number", min: -200, max: 200, unit: "px" },
  },
  {
    path: "buddy.smoothness", section: "buddy", group: "Position", label: "Follow smoothness",
    help: "How much the buddy trails behind. All the way left keeps it glued to the pointer.",
    keywords: "lag trail delay speed easing",
    control: { kind: "slider", min: 0, max: 0.95, step: 0.05, format: (v) => (v === 0 ? "Glued" : v.toFixed(2)), ends: ["Glued", "Floaty"] },
  },
  {
    path: "buddy.showStateAnimations", section: "buddy", group: "Details", label: "State animations",
    help: "Blinks when idle, pulses while listening, bubbles while thinking.", keywords: "motion animate",
    control: { kind: "toggle" },
  },
  {
    path: "buddy.showAgentBadge", section: "buddy", group: "Details", label: "Agent count badge",
    help: "A small number on the buddy while agents are running.", keywords: "agents counter",
    control: { kind: "toggle" },
  },
  {
    path: "buddy.hideInFullscreen", section: "buddy", group: "Auto-hide", label: "Hide in fullscreen apps",
    help: "Games, videos and presentations get the whole screen.", keywords: "games video presentation",
    control: { kind: "toggle" },
  },
  {
    path: "buddy.hideWhenIdle", section: "buddy", group: "Auto-hide", label: "Hide when the mouse rests",
    help: "Comes back as soon as you move the mouse.", keywords: "inactive idle timeout",
    control: { kind: "toggle" },
  },
  {
    path: "buddy.idleSeconds", section: "buddy", group: "Auto-hide", label: "Hide after",
    keywords: "seconds timeout idle", control: { kind: "number", min: 2, max: 3600, unit: "sec" },
    when: (s) => s.buddy.hideWhenIdle,
  },

  // Hotkeys
  { path: "hotkeys.voiceAsk", section: "hotkeys", group: "Ask", label: "Voice ask", help: "Opens the listening panel next to your cursor.", keywords: "speak talk microphone", control: { kind: "hotkey" } },
  {
    path: "hotkeys.voiceMode", section: "hotkeys", group: "Ask", label: "Voice hotkey behavior",
    keywords: "push to talk toggle hold",
    control: {
      kind: "segmented",
      options: [
        { value: "pushToTalk", label: "Hold to talk" },
        { value: "toggle", label: "Press to start, press to stop" },
      ],
    },
  },
  { path: "hotkeys.textAsk", section: "hotkeys", group: "Ask", label: "Text ask", help: "For when you can't talk out loud.", keywords: "type keyboard", control: { kind: "hotkey" } },
  { path: "hotkeys.circleToExplain", section: "hotkeys", group: "Screen", label: "Circle to explain", keywords: "select region lasso", control: { kind: "hotkey" } },
  { path: "hotkeys.clearAnnotations", section: "hotkeys", group: "Screen", label: "Clear annotations", help: "Removes highlights and arrows from the screen.", keywords: "erase remove drawings", control: { kind: "hotkey" } },
  { path: "hotkeys.pauseCapture", section: "hotkeys", group: "Screen", label: "Pause screen capture", keywords: "privacy screenshot", control: { kind: "hotkey" } },
  { path: "hotkeys.openAgentPanel", section: "hotkeys", group: "Agents", label: "Open agent panel", control: { kind: "hotkey" } },
  { path: "hotkeys.openApprovalInbox", section: "hotkeys", group: "Agents", label: "Open approval inbox", control: { kind: "hotkey" } },
  { path: "hotkeys.pauseAllAgents", section: "hotkeys", group: "Agents", label: "Pause all agents", keywords: "stop", control: { kind: "hotkey" } },
  { path: "hotkeys.openSettings", section: "hotkeys", group: "App", label: "Open settings", keywords: "preferences", control: { kind: "hotkey" } },

  // AI providers
  {
    path: "ai.providers", section: "ai", group: "Providers", label: "Connected providers",
    help: "API keys are stored in your system keychain, never in Helpy's settings file.",
    keywords: "anthropic claude openai gpt gemini google ollama lm studio llama.cpp local models api key endpoint",
    control: { kind: "providers" },
  },
  {
    path: "ai.routing", section: "ai", group: "Which model does what", label: "Model for each feature",
    help: "Features that aren't built yet keep your choice until they arrive.",
    keywords: "routing vision questions guidance agents orchestrator worker planning",
    control: { kind: "routing" },
  },
  {
    path: "ai.fallbackChain", section: "ai", group: "Which model does what", label: "Fallback models",
    help: "If a model fails, these are tried in order, one attempt each. They count toward the retry limit.",
    keywords: "backup failover chain order", control: { kind: "fallbackChain" },
  },
  {
    path: "ai.temperature", section: "ai", group: "Responses", label: "Temperature",
    help: "Higher is more varied. Current Claude models and OpenAI reasoning models ignore it.",
    keywords: "creativity randomness sampling",
    control: { kind: "slider", min: 0, max: 2, step: 0.1, format: (v) => v.toFixed(1), ends: ["Focused", "Varied"] },
  },
  {
    path: "ai.maxResponseTokens", section: "ai", group: "Responses", label: "Max response length",
    help: "Includes any thinking the model does before answering.",
    keywords: "max tokens output limit", control: { kind: "number", min: 256, max: 128000, unit: "tokens" },
  },
  {
    path: "ai.timeoutSecs", section: "ai", group: "Responses", label: "Give up after",
    help: "Seconds without any data from the provider before a request counts as timed out.",
    keywords: "timeout seconds wait", control: { kind: "number", min: 5, max: 600, unit: "sec" },
  },
  {
    path: "ai.customInstructions", section: "ai", group: "Responses", label: "Custom instructions",
    help: "Added to every request. Tell Helpy about your setup so answers fit it.",
    keywords: "system prompt about me context",
    control: { kind: "textarea", placeholder: "I use Windows 11 and Outlook desktop. I'm new to spreadsheets.", max: 4000 },
  },
  {
    path: "limits.maxRetries", section: "ai", group: "Retries and daily budget", label: "Retries per failed request",
    help: "Timeouts, network errors, rate limits and provider errors are retried. A bad key or missing model never is.",
    keywords: "retry attempts", control: { kind: "number", min: 0, max: 10, unit: "times" },
  },
  {
    path: "limits.backoffBaseMs", section: "ai", group: "Retries and daily budget", label: "First retry after",
    help: "Each later retry waits twice as long, up to the maximum below.",
    keywords: "backoff delay wait", control: { kind: "number", min: 100, max: 60000, unit: "ms" },
  },
  {
    path: "limits.backoffMaxMs", section: "ai", group: "Retries and daily budget", label: "Longest wait between retries",
    help: "If a provider asks Helpy to wait longer than this, the request stops instead.",
    keywords: "backoff maximum delay", control: { kind: "number", min: 100, max: 600000, unit: "ms" },
  },
  {
    path: "limits.dailyTokenBudget", section: "ai", group: "Retries and daily budget", label: "Daily token limit",
    help: "Checked before every request. 0 turns it off.",
    keywords: "budget spend tokens usage cap", control: { kind: "number", min: 0, max: 1000000000, unit: "tokens" },
  },
  {
    path: "limits.dailyCostBudget", section: "ai", group: "Retries and daily budget", label: "Daily cost limit",
    help: "Needs a price on every model you use. Leave empty to turn it off.",
    keywords: "budget spend dollars money cost cap", control: { kind: "money", emptyLabel: "Off" },
  },

  // Answer style
  {
    path: "answerStyle.detail", section: "answerStyle", group: "Answers", label: "Detail",
    keywords: "length verbose brief short long",
    control: {
      kind: "segmented",
      options: [
        { value: "brief", label: "Brief" },
        { value: "normal", label: "Normal" },
        { value: "detailed", label: "Detailed" },
      ],
    },
  },
  {
    path: "answerStyle.tone", section: "answerStyle", group: "Answers", label: "Tone",
    keywords: "voice formal casual friendly",
    control: {
      kind: "segmented",
      options: [
        { value: "casual", label: "Casual" },
        { value: "formal", label: "Formal" },
      ],
    },
  },
  {
    path: "answerStyle.screenAccess", section: "answerStyle", group: "Your screen", label: "Helpy may look at your screen",
    help: "When needed, the AI decides per question. Screenshots never include Helpy's own windows.",
    keywords: "screenshot privacy vision capture permission",
    control: {
      kind: "segmented",
      options: [
        { value: "whenNeeded", label: "When needed" },
        { value: "ask", label: "Ask me each time" },
        { value: "always", label: "Always" },
      ],
    },
  },
];

const SPEECH_LANGUAGES: Option[] = [
  { value: "auto", label: "Detect automatically" },
  { value: "en", label: "English" },
  { value: "de", label: "Deutsch" },
  { value: "es", label: "Español" },
  { value: "fr", label: "Français" },
  { value: "it", label: "Italiano" },
  { value: "nl", label: "Nederlands" },
  { value: "pl", label: "Polski" },
  { value: "pt", label: "Português" },
  { value: "hi", label: "हिन्दी" },
  { value: "ja", label: "日本語" },
  { value: "ko", label: "한국어" },
  { value: "zh", label: "中文" },
];

const secs = (v: number) => (v === 0 ? "Off" : `${v.toFixed(1)} s`);

FIELDS.push(
  // Voice input
  {
    path: "voiceInput.engine", section: "voiceInput", group: "Speech recognition", label: "Engine",
    help: "On this computer is private and free. Cloud engines send your recording to that service.",
    keywords: "speech to text stt whisper transcription cloud",
    control: {
      kind: "segmented",
      options: [
        { value: "whisper", label: "On this computer" },
        { value: "openAi", label: "OpenAI" },
        { value: "deepgram", label: "Deepgram" },
      ],
    },
  },
  {
    path: "voiceInput.whisperModel", section: "voiceInput", group: "Speech recognition", label: "Whisper model",
    help: "Bigger models understand more but take longer. Base is a good start.",
    keywords: "whisper model download local offline size",
    control: { kind: "whisperModels" },
    when: (s) => s.voiceInput.engine === "whisper" || s.voiceInput.wakeWord,
  },
  {
    path: "voiceInput.openaiProviderId", section: "voiceInput", group: "Speech recognition", label: "OpenAI account",
    help: "Uses the API key of this provider from AI providers.", keywords: "openai key transcription",
    control: { kind: "providerSelect" }, when: (s) => s.voiceInput.engine === "openAi",
  },
  {
    path: "voiceInput.openaiModel", section: "voiceInput", group: "Speech recognition", label: "Transcription model",
    keywords: "whisper-1 model", control: { kind: "text", placeholder: "whisper-1" },
    when: (s) => s.voiceInput.engine === "openAi",
  },
  {
    path: "voiceInput.deepgramModel", section: "voiceInput", group: "Speech recognition", label: "Deepgram",
    help: "The key is stored in your system keychain.", keywords: "deepgram key nova model",
    control: { kind: "deepgram" }, when: (s) => s.voiceInput.engine === "deepgram",
  },
  {
    path: "voiceInput.microphone", section: "voiceInput", group: "Microphone", label: "Microphone",
    help: "Speak to check the level.", keywords: "input device mic level meter",
    control: { kind: "micPicker" },
  },
  {
    path: "voiceInput.noiseSuppression", section: "voiceInput", group: "Microphone", label: "Noise suppression",
    help: "Filters out fans, keyboards and background hum before transcribing.", keywords: "noise background filter rnnoise",
    control: { kind: "toggle" },
  },
  {
    path: "voiceInput.language", section: "voiceInput", group: "Listening", label: "Language you speak",
    keywords: "input language locale", control: { kind: "select", options: SPEECH_LANGUAGES },
  },
  {
    path: "voiceInput.silenceSeconds", section: "voiceInput", group: "Listening", label: "Stop after silence",
    help: "In toggle mode and after the wake word. Push-to-talk stops when you let go.",
    keywords: "silence auto stop pause timeout vad",
    control: { kind: "slider", min: 0, max: 5, step: 0.5, format: secs, ends: ["Off", "5 s"] },
  },
  {
    path: "voiceInput.wakeWord", section: "voiceInput", group: "Listening", label: "Wake word",
    help: "Listens all the time for the phrase below, on this computer only. Needs the Whisper model.",
    keywords: "hey helpy hands free always listening hotword",
    control: { kind: "toggle" },
  },
  {
    path: "voiceInput.wakePhrase", section: "voiceInput", group: "Listening", label: "Wake phrase",
    help: "Two or three distinct words work best.", keywords: "hotword phrase",
    control: { kind: "text", placeholder: "hey helpy" }, when: (s) => s.voiceInput.wakeWord,
  },

  // Visual guidance
  {
    path: "guidance.highlightColor", section: "guidance", group: "Look", label: "Colour",
    help: "Used for highlights, arrows and pointers.", keywords: "highlight colour color red accent",
    control: { kind: "color", swatches: ["#e5484d", "#ff8a00", "#ffd60a", "#30c85e", "#3e8bff", "#a855f7"] },
  },
  {
    path: "guidance.highlightThickness", section: "guidance", group: "Look", label: "Line thickness",
    keywords: "highlight outline width stroke", control: { kind: "slider", min: 1, max: 8, step: 1, format: px },
  },
  {
    path: "guidance.glow", section: "guidance", group: "Look", label: "Glow",
    help: "A soft halo that makes marks easier to spot on busy screens.", keywords: "highlight shadow halo",
    control: { kind: "toggle" },
  },
  {
    path: "guidance.dim", section: "guidance", group: "Look", label: "Dim the rest of the screen",
    help: "Darkens everything outside a highlight.", keywords: "dim darken spotlight focus background",
    control: { kind: "slider", min: 0, max: 0.7, step: 0.05, format: (v) => (v === 0 ? "Off" : pct(v)), ends: ["Off", "Dark"] },
  },
  {
    path: "guidance.labelStyle", section: "guidance", group: "Look", label: "Labels",
    keywords: "label style pointer bubble",
    control: {
      kind: "segmented",
      options: [
        { value: "accent", label: "Coloured" },
        { value: "dark", label: "Dark" },
      ],
    },
  },
  {
    path: "guidance.arrowStyle", section: "guidance", group: "Look", label: "Arrows",
    keywords: "arrow curved straight",
    control: {
      kind: "segmented",
      options: [
        { value: "curved", label: "Curved" },
        { value: "straight", label: "Straight" },
      ],
    },
  },
  {
    path: "guidance.animationSpeed", section: "guidance", group: "Motion", label: "Animation speed",
    keywords: "animation fast slow", when: (s) => !s.guidance.reduceMotion,
    control: { kind: "slider", min: 0.5, max: 2, step: 0.25, format: (v) => `${v.toFixed(2)}×`, ends: ["Slower", "Faster"] },
  },
  {
    path: "guidance.reduceMotion", section: "guidance", group: "Motion", label: "Reduce motion",
    help: "Marks appear without drawing in or pulsing. Helpy also follows your system setting.",
    keywords: "animation accessibility still", control: { kind: "toggle" },
  },
  {
    path: "guidance.advance", section: "guidance", group: "Walkthroughs", label: "Next step",
    help: "Clicking near the highlighted spot needs Accessibility permission on macOS. Without it, use Next.",
    keywords: "auto advance click next step walkthrough",
    control: {
      kind: "segmented",
      options: [
        { value: "onClick", label: "When I click the spot" },
        { value: "nextButton", label: "When I press Next" },
      ],
    },
  },
  {
    path: "guidance.cardPosition", section: "guidance", group: "Walkthroughs", label: "Step card",
    help: "The small card with Next, Repeat and Stop.", keywords: "step card position top bottom",
    control: {
      kind: "segmented",
      options: [
        { value: "nearTarget", label: "Next to the spot" },
        { value: "top", label: "Top" },
        { value: "bottom", label: "Bottom" },
      ],
    },
  },
  {
    path: "guidance.annotationSeconds", section: "guidance", group: "Walkthroughs", label: "Hide marks after",
    help: "0 keeps them until you've done the step. Repeat shows them again.", keywords: "annotation duration timeout fade",
    control: { kind: "number", min: 0, max: 600, unit: "sec" },
  },
  {
    path: "guidance.maxSteps", section: "guidance", group: "Walkthroughs", label: "Most steps per walkthrough",
    help: "Helpy stops and answers in text after this many.", keywords: "limit steps runaway",
    control: { kind: "number", min: 1, max: 30, unit: "steps" },
  },
  {
    path: "guidance.showCoordinates", section: "guidance", group: "Troubleshooting", label: "Show raw coordinates",
    help: "Prints the model's numbers next to each mark, to check where it thinks things are.",
    keywords: "debug coordinates pixels position", control: { kind: "toggle" },
  },

  // Voice output
  {
    path: "voiceOutput.voiceGuidance", section: "voiceOutput", group: "Speaking", label: "Read answers aloud",
    help: "For spoken questions and walkthrough steps. Also in the tray menu and on the step card. Press Esc to stop talking.",
    keywords: "voice guidance tts speak read aloud mute", control: { kind: "toggle" },
  },
  {
    path: "voiceOutput.readAloud", section: "voiceOutput", group: "Speaking", label: "What to read",
    keywords: "steps full answer",
    control: {
      kind: "segmented",
      options: [
        { value: "fullAnswers", label: "Full answers" },
        { value: "stepsOnly", label: "Step instructions only" },
      ],
    },
  },
  {
    path: "voiceOutput.announceAgents", section: "voiceOutput", group: "Speaking", label: "Announce finished agents",
    help: "A short spoken summary when an agent finishes. Takes effect once agents arrive.", keywords: "agents announcements notifications",
    control: { kind: "toggle" },
  },
  {
    path: "voiceOutput.engine", section: "voiceOutput", group: "Voice", label: "Engine",
    help: "System voices need no download. Piper voices sound more natural and run on this computer.",
    keywords: "tts text to speech piper openai system voice",
    control: {
      kind: "segmented",
      options: [
        { value: "system", label: "System voices" },
        { value: "piper", label: "Piper", requires: "piper" },
        { value: "openAi", label: "OpenAI" },
      ],
    },
  },
  {
    path: "voiceOutput.systemVoice", section: "voiceOutput", group: "Voice", label: "Voice",
    keywords: "system voice", control: { kind: "systemVoice" }, when: (s) => s.voiceOutput.engine === "system",
  },
  {
    path: "voiceOutput.piperVoice", section: "voiceOutput", group: "Voice", label: "Piper voice",
    help: "Download a voice to use it. Most are 20 to 120 MB.", keywords: "piper voice download",
    control: { kind: "piperVoice" }, when: (s) => s.voiceOutput.engine === "piper",
  },
  {
    path: "voiceOutput.openaiProviderId", section: "voiceOutput", group: "Voice", label: "OpenAI account",
    help: "Uses the API key of this provider from AI providers.", keywords: "openai key",
    control: { kind: "providerSelect" }, when: (s) => s.voiceOutput.engine === "openAi",
  },
  {
    path: "voiceOutput.openaiModel", section: "voiceOutput", group: "Voice", label: "Speech model",
    keywords: "tts-1 model", control: { kind: "text", placeholder: "tts-1" }, when: (s) => s.voiceOutput.engine === "openAi",
  },
  {
    path: "voiceOutput.openaiVoice", section: "voiceOutput", group: "Voice", label: "OpenAI voice",
    keywords: "alloy nova voice",
    control: {
      kind: "select",
      options: ["alloy", "echo", "fable", "onyx", "nova", "shimmer"].map((v) => ({ value: v, label: v[0].toUpperCase() + v.slice(1) })),
    },
    when: (s) => s.voiceOutput.engine === "openAi",
  },
  {
    path: "voiceOutput.speed", section: "voiceOutput", group: "Voice", label: "Speed", keywords: "rate fast slow",
    control: { kind: "slider", min: 0.5, max: 2, step: 0.05, format: (v) => `${v.toFixed(2)}×`, ends: ["Slower", "Faster"] },
  },
  {
    path: "voiceOutput.volume", section: "voiceOutput", group: "Voice", label: "Volume", keywords: "loud quiet",
    control: { kind: "slider", min: 0, max: 1, step: 0.05, format: (v) => `${Math.round(v * 100)}%` },
  },
);

export function searchFields(query: string): Field[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  return FIELDS.filter((f) => {
    const section = SECTIONS.find((s) => s.id === f.section)!.title;
    const hay = `${f.label} ${f.help ?? ""} ${f.keywords ?? ""} ${f.group} ${section}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}
