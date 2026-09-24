// Everything the settings page knows about each setting: where it lives,
// what it's called, and how to edit it. Rendering and search both read this.
// Types and validation come from the Rust schema (src-tauri/src/settings).

import type { Settings } from "../bindings/Settings";
import type { SettingPath } from "../lib/ipc";

export type SectionId = "general" | "buddy" | "hotkeys";

export const SECTIONS: { id: SectionId; title: string; blurb: string }[] = [
  { id: "general", title: "General", blurb: "Startup, appearance and language." },
  { id: "buddy", title: "Cursor buddy", blurb: "The small character that rides along with your mouse." },
  { id: "hotkeys", title: "Hotkeys", blurb: "Shortcuts that work from any app. Click one to change it." },
];

type Option = { value: string; label: string };

export type Control =
  | { kind: "toggle" }
  | { kind: "segmented"; options: Option[] }
  | { kind: "select"; options: Option[] }
  | { kind: "slider"; min: number; max: number; step: number; format: (v: number) => string; ends?: [string, string] }
  | { kind: "number"; min: number; max: number; unit: string }
  | { kind: "hotkey" }
  | { kind: "buddyStyle" };

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
];

export function searchFields(query: string): Field[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  return FIELDS.filter((f) => {
    const section = SECTIONS.find((s) => s.id === f.section)!.title;
    const hay = `${f.label} ${f.help ?? ""} ${f.keywords ?? ""} ${f.group} ${section}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}
