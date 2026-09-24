// The settings registry: one entry per setting, used for both rendering and search.
// Types and defaults come from the Rust schema (src/bindings); this file only adds
// the human side: labels, help text, search keywords, and which control to show.
//
// Settings that belong to later phases are listed with `phase` so search can still
// find them and the page can say when they arrive.

import type { ReactNode } from "react";
import type { SectionId } from "../bindings/SectionId";
import type { HotkeyAction } from "../bindings/HotkeyAction";
import { NumberField, Segmented, Select, Slider, Toggle } from "./controls/controls";
import { HotkeyField } from "./controls/HotkeyField";
import { BuddyStylePicker, CustomBuddyUpload, OverlayCheckButton, ResetAllButton } from "./sections/extras";
import type { IconName } from "./icons";

export type SectionKey =
  | "general" | "profiles" | "buddy" | "hotkeys"
  | "providers" | "answerStyle" | "guidance" | "circle"
  | "voiceInput" | "voiceOutput"
  | "agents" | "connectors"
  | "privacy" | "usage" | "advanced";

export interface SectionDef {
  key: SectionKey;
  title: string;
  blurb: string;
  icon: IconName;
  nav: string;
  /** Backend section id for "reset to defaults", when the section has live settings. */
  resetId?: SectionId;
  /** Phase that makes the whole section available, if none of it is live yet. */
  phase?: number;
}

export interface SettingDef {
  id: string;
  section: SectionKey;
  group: string;
  label: string;
  help?: string;
  keywords?: string;
  /** Later phase that delivers this setting. Absent means it works now. */
  phase?: number;
  /** Put the control under the label instead of beside it. */
  wide?: boolean;
  control?: () => ReactNode;
  /** Only shown when this returns true (for example, dependent settings). */
  when?: (get: <T>(p: string) => T) => boolean;
}

export const NAV_GROUPS = ["Everyday", "Assistant", "Voice", "Agents", "Trust and system"];

export const SECTIONS: SectionDef[] = [
  { key: "general", nav: "Everyday", icon: "sliders", title: "General", blurb: "Startup, theme, and language.", resetId: "general" },
  { key: "profiles", nav: "Everyday", icon: "layers", title: "Behavior profiles", blurb: "Switch between bundles of behavior from the tray.", phase: 9 },
  { key: "buddy", nav: "Everyday", icon: "buddy", title: "Cursor buddy", blurb: "The little companion that rides next to your pointer. It never blocks a click.", resetId: "buddy" },
  { key: "hotkeys", nav: "Everyday", icon: "keyboard", title: "Hotkeys", blurb: "Global shortcuts that work from any app. Click a shortcut to record a new one.", resetId: "hotkeys" },
  { key: "providers", nav: "Assistant", icon: "chip", title: "AI providers", blurb: "Cloud and local models, routing, and fallbacks.", phase: 2 },
  { key: "answerStyle", nav: "Assistant", icon: "chat", title: "Answer style", blurb: "How much detail, which tone, and when Helpy may look at your screen.", phase: 2 },
  { key: "guidance", nav: "Assistant", icon: "target", title: "Visual guidance", blurb: "Highlights, pointers, arrows, and step-by-step walkthroughs.", phase: 4 },
  { key: "circle", nav: "Assistant", icon: "lasso", title: "Circle to explain", blurb: "Select anything on screen and ask about it.", phase: 5 },
  { key: "voiceInput", nav: "Voice", icon: "mic", title: "Voice input", blurb: "Speech recognition, microphone, and wake word.", phase: 3 },
  { key: "voiceOutput", nav: "Voice", icon: "speaker", title: "Voice output", blurb: "Spoken answers and agent announcements.", resetId: "voiceOutput" },
  { key: "agents", nav: "Agents", icon: "agents", title: "Agents", blurb: "Limits, budgets, approvals, floating cards, templates, and triggers.", phase: 6 },
  { key: "connectors", nav: "Agents", icon: "plug", title: "Connectors", blurb: "Gmail, Notion, Microsoft 365, Slack, GitHub, and MCP servers.", phase: 7 },
  { key: "privacy", nav: "Trust and system", icon: "shield", title: "Privacy", blurb: "What Helpy captures, what it sends, and to whom.", phase: 9 },
  { key: "usage", nav: "Trust and system", icon: "meter", title: "Usage", blurb: "Tokens and estimated cost by provider, feature, agent, and day.", phase: 9 },
  { key: "advanced", nav: "Trust and system", icon: "wrench", title: "Advanced", blurb: "Logs, full reset, and onboarding." },
];

const pct = (v: number) => `${Math.round(v * 100)}%`;
const px = (v: number) => `${v > 0 ? "+" : ""}${v} px`;

const LANGS = [
  { value: "en", label: "English" }, { value: "de", label: "Deutsch" }, { value: "es", label: "Español" },
  { value: "fr", label: "Français" }, { value: "it", label: "Italiano" }, { value: "pt-BR", label: "Português (Brasil)" },
  { value: "nl", label: "Nederlands" }, { value: "pl", label: "Polski" }, { value: "ja", label: "日本語" },
  { value: "ko", label: "한국어" }, { value: "zh-CN", label: "简体中文" }, { value: "hi", label: "हिन्दी" },
];

function hotkey(action: HotkeyAction, label: string, help?: string, keywords?: string): SettingDef {
  return { id: `hotkeys.${action}`, section: "hotkeys", group: "Shortcuts", label, help, keywords, control: () => <HotkeyField action={action} /> };
}

/** Compact helper for settings that arrive later. */
function later(section: SectionKey, phase: number, group: string, items: (string | [string, string])[]): SettingDef[] {
  return items.map((it) => {
    const [label, help] = Array.isArray(it) ? it : [it, undefined];
    return { id: `${section}:${label}`, section, group, label, help, phase };
  });
}

export const SETTINGS: SettingDef[] = [
  // ---------------------------------------------------------------- General
  { id: "general.launchAtLogin", section: "general", group: "Startup", label: "Launch at login", help: "Recommended if you use scheduled agents, since agents only run while Helpy is open.", keywords: "autostart boot startup", control: () => <Toggle path="general.launchAtLogin" /> },
  { id: "general.startMinimized", section: "general", group: "Startup", label: "Start minimized to tray", help: "Open quietly in the tray instead of showing this window.", keywords: "hidden background", control: () => <Toggle path="general.startMinimized" /> },
  { id: "general.checkForUpdates", section: "general", group: "Startup", label: "Check for updates automatically", control: () => <Toggle path="general.checkForUpdates" /> },
  { id: "general.theme", section: "general", group: "Appearance", label: "App theme", help: "System follows your OS light or dark setting.", keywords: "dark mode light mode color", control: () => <Segmented path="general.theme" options={[{ value: "system", label: "System" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} /> },
  { id: "general.interfaceLanguage", section: "general", group: "Language", label: "Interface language", help: "Menus and settings. More translations land before 1.0; untranslated text shows in English.", keywords: "locale translation", control: () => <Select path="general.interfaceLanguage" options={LANGS} /> },
  { id: "general.responseLanguage", section: "general", group: "Language", label: "AI response language", help: "Auto answers in the language you spoke or typed.", keywords: "reply answer locale", control: () => <Select path="general.responseLanguage" options={[{ value: "auto", label: "Auto (match my language)" }, ...LANGS]} /> },

  // ---------------------------------------------------------------- Profiles
  ...later("profiles", 9, "Profiles", [
    ["Beginner", "Detailed step-by-step answers, slower voice, always highlight, auto-advance steps."],
    ["Expert", "Short answers, no voice, pointers only."],
    ["Quiet", "No voice, no buddy animations, minimal overlays, agent news as notifications only."],
    "Create, rename, duplicate, and delete your own profiles",
  ]),

  // ---------------------------------------------------------------- Buddy
  { id: "buddy.enabled", section: "buddy", group: "Buddy", label: "Show cursor buddy", help: "Also in the tray menu.", keywords: "companion character mascot on off", control: () => <Toggle path="buddy.enabled" /> },
  { id: "buddy.style", section: "buddy", group: "Buddy", label: "Style", wide: true, keywords: "character look skin image svg png upload custom", control: () => <BuddyStylePicker /> },
  { id: "buddy.customImage", section: "buddy", group: "Buddy", label: "Your own buddy", help: "SVG or PNG, up to 2 MB. Square images with a transparent background look best.", keywords: "upload custom image svg png", control: () => <CustomBuddyUpload /> },
  { id: "buddy.size", section: "buddy", group: "Look", label: "Size", keywords: "scale big small", control: () => <Slider path="buddy.size" min={24} max={96} step={2} format={(v) => `${v} px`} /> },
  { id: "buddy.opacity", section: "buddy", group: "Look", label: "Opacity", keywords: "transparency", control: () => <Slider path="buddy.opacity" min={0.2} max={1} step={0.05} format={pct} /> },
  { id: "buddy.showStateAnimations", section: "buddy", group: "Look", label: "State animations", help: "Listening ring, thinking dots, talking mouth, and idle blinking.", keywords: "motion animate", control: () => <Toggle path="buddy.showStateAnimations" /> },
  { id: "buddy.showAgentBadge", section: "buddy", group: "Look", label: "Running agents badge", help: "A small number showing how many agents are working.", keywords: "count agents", control: () => <Toggle path="buddy.showAgentBadge" /> },
  { id: "buddy.offsetX", section: "buddy", group: "Following", label: "Horizontal offset", help: "Distance from the pointer tip. Near a screen edge the buddy flips to the other side.", keywords: "position x distance", control: () => <Slider path="buddy.offsetX" min={-120} max={120} format={px} /> },
  { id: "buddy.offsetY", section: "buddy", group: "Following", label: "Vertical offset", keywords: "position y distance", control: () => <Slider path="buddy.offsetY" min={-120} max={120} format={px} /> },
  { id: "buddy.smoothness", section: "buddy", group: "Following", label: "Follow smoothness", help: "Locked stays glued to the pointer. Higher values glide behind it.", keywords: "lag speed smooth glide", control: () => <Slider path="buddy.smoothness" min={0} max={1} step={0.05} format={(v) => (v === 0 ? "Locked" : pct(v))} ends={["Locked", "Floaty"]} /> },
  { id: "buddy.hideInFullscreen", section: "buddy", group: "Hiding", label: "Hide in fullscreen apps", help: "Games, videos, and presentations. Detection works on Windows now; macOS and Linux follow in Phase 9.", keywords: "auto hide game video presentation", control: () => <Toggle path="buddy.hideInFullscreen" /> },
  { id: "buddy.hideWhenIdle", section: "buddy", group: "Hiding", label: "Hide when the mouse is still", keywords: "idle inactivity auto hide", control: () => <Toggle path="buddy.hideWhenIdle" /> },
  { id: "buddy.idleSeconds", section: "buddy", group: "Hiding", label: "Hide after", help: "3 to 600 seconds of no mouse movement.", keywords: "idle timeout seconds", when: (get) => get<boolean>("buddy.hideWhenIdle"), control: () => <NumberField path="buddy.idleSeconds" min={3} max={600} suffix="seconds" /> },
  { id: "buddy.overlayCheck", section: "buddy", group: "Displays", label: "Check overlay alignment", help: "Draws a frame on every monitor so you can confirm scaling is right. It clears itself, or press Esc.", keywords: "dpi monitor display scaling calibrate test", control: () => <OverlayCheckButton /> },

  // ---------------------------------------------------------------- Hotkeys
  hotkey("voiceAsk", "Voice ask", "Speak a question. Hold or tap depending on the mode below.", "talk speak microphone"),
  { id: "hotkeys.voiceMode", section: "hotkeys", group: "Shortcuts", label: "Voice hotkey mode", help: "Push to talk listens while held. Toggle starts on the first press and stops on the second.", keywords: "push to talk ptt hold toggle", control: () => <Segmented path="hotkeys.voiceMode" options={[{ value: "pushToTalk", label: "Push to talk" }, { value: "toggle", label: "Toggle" }]} /> },
  hotkey("textAsk", "Text ask", "Type a question when you can't talk out loud.", "type keyboard"),
  hotkey("circleToExplain", "Circle to explain", "Select part of the screen to ask about it.", "select lasso region"),
  hotkey("clearAnnotations", "Clear annotations", "Esc also works whenever something is drawn on screen.", "remove highlights escape"),
  hotkey("pauseCapture", "Pause screen capture", undefined, "privacy screenshot stop"),
  hotkey("openAgentPanel", "Open agent panel"),
  hotkey("openApprovalInbox", "Open approval inbox", undefined, "approve reject"),
  hotkey("pauseAllAgents", "Pause all agents", undefined, "stop halt"),
  hotkey("openSettings", "Open settings", undefined, "preferences"),

  // ---------------------------------------------------------------- Providers
  ...later("providers", 2, "Providers", [
    ["Add a provider", "Anthropic, OpenAI, Google Gemini, Ollama, LM Studio, llama.cpp server, or any OpenAI-compatible endpoint."],
    ["Detect local models", "Finds a running Ollama or LM Studio and lists its models."],
    "Test connection",
    ["Vision support per model", "Detected where possible, and you can override it."],
  ]),
  ...later("providers", 2, "Routing", [
    ["Model per feature", "Separate models for voice Q&A, visual guidance, circle to explain, agent planning, orchestrators, and workers."],
    "Fallback chain order",
  ]),
  ...later("providers", 2, "Requests", ["Temperature", "Maximum response length", "Request timeout", ["Custom instructions", "Added to every request, for example: I use Windows 11 and Outlook desktop."]]),

  // ---------------------------------------------------------------- Answer style
  ...later("answerStyle", 2, "Answers", [
    ["Detail level", "Brief, normal, or detailed."],
    ["Tone", "Casual or formal."],
    ["Looking at the screen", "Always, ask each time, or only when needed."],
  ]),

  // ---------------------------------------------------------------- Guidance
  ...later("guidance", 4, "Highlights", ["Highlight color", "Highlight thickness", "Glow", "Dim the rest of the screen"]),
  ...later("guidance", 4, "Labels and arrows", ["Label font size", "Label color", "Label position", "Arrow style (straight or curved)", "Arrow color", "Arrow thickness", "Animated arrows"]),
  ...later("guidance", 4, "Walkthroughs", [
    ["Auto-advance", "Move to the next step when you click the target, or wait for Next."],
    "How long annotations stay on screen",
    "Step card position",
    "Animation speed",
    "Reduce motion",
    ["Precision snapping", "Snap to real buttons using the accessibility tree."],
    ["Do it for me", "Off by default. Every click needs your confirmation."],
    ["Debug coordinates", "Show raw AI coordinates next to snapped ones."],
  ]),

  // ---------------------------------------------------------------- Circle
  ...later("circle", 5, "Selection", [
    ["Default selection shape", "Rectangle or freehand."],
    ["Action after selecting", "Explain, copy text, translate, send to an agent, or show a menu."],
    ["Diagram label detail", "How much each labeled part says."],
  ]),

  // ---------------------------------------------------------------- Voice input
  ...later("voiceInput", 3, "Recognition", [
    ["Speech-to-text engine", "Local Whisper or a cloud provider."],
    ["Whisper models", "Download, delete, and choose a model size, with disk sizes shown."],
    ["Input language", "Auto-detect or fixed."],
  ]),
  ...later("voiceInput", 3, "Microphone", ["Microphone", "Input level meter", ["Auto-stop after silence", "Stop listening after a few quiet seconds."], "Noise suppression"]),
  ...later("voiceInput", 3, "Wake word", [["Wake word", "Off by default."], "Wake phrase"]),

  // ---------------------------------------------------------------- Voice output
  { id: "voiceOutput.guidanceEnabled", section: "voiceOutput", group: "Speaking", label: "Voice guidance", help: "Read answers and steps aloud. Speech itself arrives in Phase 3; this switch is already shared with the tray.", keywords: "speak read aloud tts", control: () => <Toggle path="voiceOutput.guidanceEnabled" /> },
  ...later("voiceOutput", 3, "Speaking", [
    ["Voice engine", "OS voices, local Piper voices, or a cloud voice."],
    "Voice",
    "Speed",
    "Volume",
    "Play sample",
    ["What to read aloud", "Full answers, or step instructions only."],
    "Spoken agent announcements",
  ]),

  // ---------------------------------------------------------------- Agents
  ...later("agents", 6, "Running and approvals", [
    "Maximum agents running at once",
    ["Maximum sub-agents per orchestrator", "Default 5."],
    ["Default run mode", "Ask, parallel, or sequential."],
    ["Plan card confirmation", "Always, only with side effects, or never for saved templates."],
    ["Approval rules per action", "Always allow, always ask, or never allow."],
  ]),
  ...later("agents", 6, "Tools and access", [
    "Approved folders",
    "Backup retention for changed files",
    ["Shell command policy", "Never, ask every time, or an allowlist of safe commands."],
    "Enable or disable each tool",
    ["External agent runners", "CLI agents like Claude Code, with their paths and approved repo folders."],
  ]),
  ...later("agents", 6, "Limits and retries", [
    "Time limit per agent",
    ["Budgets", "Per agent, per batch, and per day, in tokens or estimated cost."],
    ["Max retries per failed step", "Default 3."],
    "Backoff base and maximum delay",
    ["On final failure", "Stop the agent, or pause and ask me."],
    ["Max steps and tool calls", "Defaults 25 steps and 50 tool calls."],
    ["Stuck detection", "Repeat threshold 3, no-progress threshold 8 steps."],
    ["Trigger auto-pause and rate limit", "Pause after 3 failed runs in a row; at most 10 runs per hour."],
  ]),
  ...later("agents", 6, "Floating agent cards", [
    "Show floating cards",
    "Card position and monitor",
    "Cards shown before collapsing into pills",
    "Card opacity and size",
    "Auto-play spoken status",
    "How long Done cards stay",
    "Cards for scheduled and triggered agents",
  ]),
  ...later("agents", 8, "Templates, triggers, notifications", [
    "Templates and voice trigger phrases",
    "Scheduled and event triggers",
    ["Notifications", "OS notification, buddy badge, sound, spoken summary."],
    "History retention",
  ]),

  // ---------------------------------------------------------------- Connectors
  ...later("connectors", 7, "Accounts", [
    ["Gmail, Google Calendar, Google Drive", "Connect with OAuth in your browser. Tokens stay in the OS keychain."],
    "Notion",
    "Microsoft Outlook and Calendar",
    "Slack",
    "GitHub",
    ["Permission level", "Read-only (default) or read and write, per connector."],
    ["Action rules", "For example, Gmail send always asks and delete is never allowed."],
    ["OAuth client", "Bundled client ID or your own Google Cloud client ID."],
  ]),
  ...later("connectors", 7, "MCP servers", [["Add an MCP server", "Local command or remote URL."], "Per-tool enable toggles"]),

  // ---------------------------------------------------------------- Privacy
  ...later("privacy", 9, "Capture", [
    ["Screen capture indicator", "On by default."],
    ["App blocklist", "Helpy never captures while these apps are focused."],
    "Blur password fields before sending",
  ]),
  ...later("privacy", 9, "Data", [
    "Conversation history and retention",
    "Clear all history",
    ["Offline mode", "Local models and speech only, with all network calls blocked."],
    ["Who receives your data", "A plain list of providers that get screenshots and connector data."],
  ]),

  // ---------------------------------------------------------------- Usage
  ...later("usage", 9, "Spending", ["Tokens and cost by provider, feature, agent, and day", ["Monthly budget", "Warn near the limit and optionally pause all agents."]]),

  // ---------------------------------------------------------------- Advanced
  { id: "advanced.resetAll", section: "advanced", group: "Reset", label: "Reset all settings", help: "Every section goes back to its defaults. Your uploaded buddy image is kept.", keywords: "factory defaults restore", control: () => <ResetAllButton /> },
  ...later("advanced", 9, "Diagnostics", ["Open log folder", "Log level", "Re-run onboarding"]),
];

export function sectionOf(key: SectionKey): SectionDef {
  return SECTIONS.find((s) => s.key === key)!;
}

/** Simple ranked search over label, keywords, help, and section title. */
export function searchSettings(query: string): SettingDef[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return [];
  const scored = SETTINGS.map((s) => {
    const sec = sectionOf(s.section).title.toLowerCase();
    const label = s.label.toLowerCase();
    const hay = `${label} ${s.keywords ?? ""} ${s.help ?? ""} ${sec} ${s.group}`.toLowerCase();
    if (!terms.every((t) => hay.includes(t))) return null;
    let score = 0;
    for (const t of terms) {
      if (label.startsWith(t)) score += 6;
      else if (label.includes(t)) score += 4;
      else if ((s.keywords ?? "").includes(t)) score += 2;
      else score += 1;
    }
    if (!s.phase) score += 1;
    return { s, score };
  }).filter(Boolean) as { s: SettingDef; score: number }[];
  return scored.sort((a, b) => b.score - a.score).map((x) => x.s);
}
