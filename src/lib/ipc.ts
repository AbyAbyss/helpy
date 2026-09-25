// Typed wrappers around Helpy's Rust commands and events.
import { invoke } from "@tauri-apps/api/core";
import type { AskStatus } from "../bindings/AskStatus";
import type { CardView } from "../bindings/CardView";
import type { CircleAction } from "../bindings/CircleAction";
import type { AgentList } from "../bindings/AgentList";
import type { Answer } from "../bindings/Answer";
import type { Plan } from "../bindings/Plan";
import type { RunMode } from "../bindings/RunMode";
import type { LocalServer } from "../bindings/LocalServer";
import type { ModelInfo } from "../bindings/ModelInfo";
import type { ProviderConfig } from "../bindings/ProviderConfig";
import type { UsageToday } from "../bindings/UsageToday";
import type { FieldError } from "../bindings/FieldError";
import type { HotkeyStatus } from "../bindings/HotkeyStatus";
import type { PiperVoice } from "../bindings/PiperVoice";
import type { PlatformInfo } from "../bindings/PlatformInfo";
import type { SystemVoice } from "../bindings/SystemVoice";
import type { VoiceSupport } from "../bindings/VoiceSupport";
import type { WhisperModel } from "../bindings/WhisperModel";
import type { Settings } from "../bindings/Settings";
import type { Template } from "../bindings/Template";
import type { TemplateInfo } from "../bindings/TemplateInfo";
import type { ToolGroup } from "../bindings/ToolGroup";
import type { TriggerStatus } from "../bindings/TriggerStatus";
import type { BrowserInfo } from "../bindings/BrowserInfo";
import type { CoderInfo } from "../bindings/CoderInfo";
import type { UsageHistory } from "../bindings/UsageHistory";
import type { Permissions } from "../bindings/Permissions";
import type { Connection } from "../bindings/Connection";
import type { ConnectorInfo } from "../bindings/ConnectorInfo";
import type { ImportReport } from "../bindings/ImportReport";
import type { Preset } from "../bindings/Preset";
import type { ServerStatus } from "../bindings/ServerStatus";
import type { ToolInfo } from "../bindings/ToolInfo";

export const EVENTS = {
  settingsChanged: "settings://changed",
  hotkeyStatus: "hotkeys://status",
  cursor: "overlay://cursor",
  buddyVisible: "buddy://visible",
  buddyImageChanged: "buddy://image-changed",
  openSection: "settings://open-section",
  askShown: "ask://shown",
  ask: "ask://event",
  voiceState: "voice://state",
  voiceLevel: "voice://level",
  voicePartial: "voice://partial",
  voiceMeter: "voice://meter",
  speaking: "voice://speaking",
  speakError: "voice://speak-error",
  download: "voice://download",
  guideMarks: "guide://marks",
  guideCard: "guide://card",
  clearAnnotations: "overlay://clear",
  circle: "circle://event",
  agentUpdate: "agents://update",
  agentRemoved: "agents://removed",
  agentLive: "agents://live",
  agentBatch: "agents://batch",
  agentCount: "agents://count",
  agentPlan: "agents://plan",
  agentsFocus: "agents://focus",
  connectorsChanged: "connectors://changed",
  mcpChanged: "mcp://changed",
} as const;

type Section = keyof Settings;

/** Every setting as "section.field", checked against the Rust schema. */
export type SettingPath = {
  [S in Section]: { [K in keyof Settings[S] & string]: `${S}.${K}` }[keyof Settings[S] & string];
}[Section];

export type ValueAt<P extends SettingPath> = P extends `${infer S extends Section}.${infer K}`
  ? K extends keyof Settings[S]
    ? Settings[S][K]
    : never
  : never;

export function getValue<P extends SettingPath>(s: Settings, path: P): ValueAt<P> {
  const [section, key] = path.split(".") as [Section, string];
  return (s[section] as Record<string, unknown>)[key] as ValueAt<P>;
}

export const api = {
  getSettings: () => invoke<Settings>("settings_get"),
  defaults: () => invoke<Settings>("settings_defaults"),
  /** Rejects with FieldError[] when the value is invalid; nothing is saved then. */
  set: <P extends SettingPath>(path: P, value: ValueAt<P>) => invoke<Settings>("settings_set", { path, value }),
  /** Replaces a whole settings group at once, for edits that span fields. */
  setGroup: <K extends Section>(key: K, value: Settings[K]) => invoke<Settings>("settings_set", { path: key, value }),
  resetSection: (section: Section) => invoke<Settings>("settings_reset_section", { section }),
  exportTo: (path: string) => invoke<void>("settings_export", { path }),
  importFrom: (path: string) => invoke<Settings>("settings_import", { path }),
  hotkeyStatus: () => invoke<HotkeyStatus[]>("hotkeys_status"),
  suspendHotkeys: () => invoke<void>("hotkeys_suspend"),
  resumeHotkeys: () => invoke<void>("hotkeys_resume"),
  overlayReady: () => invoke<boolean>("overlay_ready"),
  setCustomBuddy: (path: string) => invoke<Settings>("buddy_set_custom_image", { path }),
  customBuddy: () => invoke<string | null>("buddy_custom_image"),
  platform: () => invoke<PlatformInfo>("platform_info"),

  // AI providers. Commands that talk to a provider reject with a plain message.
  setKey: (providerId: string, key: string) => invoke<void>("ai_set_key", { providerId, key }),
  deleteKey: (providerId: string) => invoke<void>("ai_delete_key", { providerId }),
  hasKey: (providerId: string) => invoke<boolean>("ai_has_key", { providerId }),
  listModels: (provider: ProviderConfig) => invoke<ModelInfo[]>("ai_list_models", { provider }),
  testProvider: (provider: ProviderConfig) => invoke<string>("ai_test_provider", { provider }),
  detectLocal: () => invoke<LocalServer[]>("ai_detect_local"),
  usageToday: () => invoke<UsageToday>("ai_usage_today"),
  usageHistory: (days: number) => invoke<UsageHistory>("ai_usage_history", { days }),

  // The ask panel. Progress arrives as EVENTS.ask events.
  ask: (text: string) => invoke<void>("ask_send", { text }),
  askCancel: () => invoke<void>("ask_cancel"),
  askReset: () => invoke<void>("ask_reset"),
  askScreenAnswer: (id: number, allow: boolean) => invoke<void>("ask_screen_answer", { id, allow }),
  askStatus: () => invoke<AskStatus>("ask_status"),
  askHide: () => invoke<void>("ask_hide"),
  openSettingsSection: (section: string) => invoke<void>("open_settings_section", { section }),

  // Voice.
  inputDevices: () => invoke<string[]>("voice_input_devices"),
  meterStart: (device: string | null, denoise: boolean) => invoke<void>("voice_meter_start", { device, denoise }),
  meterStop: () => invoke<void>("voice_meter_stop"),
  whisperModels: () => invoke<WhisperModel[]>("voice_whisper_models"),
  whisperDownload: (name: string) => invoke<void>("voice_whisper_download", { name }),
  whisperDelete: (name: string) => invoke<void>("voice_whisper_delete", { name }),
  systemVoices: () => invoke<SystemVoice[]>("voice_system_voices"),
  piperVoices: () => invoke<PiperVoice[]>("voice_piper_voices"),
  piperInstall: (key: string) => invoke<void>("voice_piper_install", { key }),
  piperRemove: (key: string) => invoke<void>("voice_piper_remove", { key }),
  playSample: () => invoke<void>("voice_play_sample"),
  stopSpeaking: () => invoke<void>("voice_stop_speaking"),
  setDeepgramKey: (key: string) => invoke<void>("voice_set_deepgram_key", { key }),
  hasDeepgramKey: () => invoke<boolean>("voice_has_deepgram_key"),
  voiceExpand: () => invoke<void>("voice_expand"),
  voicePillHide: () => invoke<void>("voice_pill_hide"),
  voiceCancel: () => invoke<void>("voice_cancel"),
  voiceSupport: () => invoke<VoiceSupport>("voice_support"),

  // Visual guidance. Steps arrive as EVENTS.guideMarks and EVENTS.guideCard.
  guideAction: (action: "next" | "repeat" | "stop" | "doIt" | "confirm" | "back") => invoke<void>("guide_action", { action }),
  guideCard: () => invoke<CardView | null>("guide_card"),
  /** The OS glass behind the step card: "macos", "windows", or null when opaque. */
  guideCardGlass: () => invoke<"macos" | "windows" | null>("guide_card_glass"),

  // Circle to explain. Progress arrives as EVENTS.circle events.
  /** Rejects with a message when the selection can't be used (too small). */
  circleSelect: (points: [number, number][]) => invoke<void>("circle_select", { points }),
  circleAction: (action: CircleAction, question: string | null = null) => invoke<void>("circle_action", { action, question }),
  circleClose: () => invoke<void>("circle_close"),
  circleCopy: (text: string) => invoke<void>("circle_copy", { text }),
  circleToAgent: (task: string) => invoke<void>("circle_to_agent", { task }),

  // Agents. Changes arrive as EVENTS.agentUpdate and friends.
  agents: () => invoke<AgentList>("agents_list"),
  agentAnswer: (id: string, answer: Answer) => invoke<void>("agents_answer", { id, answer }),
  agentPause: (id: string) => invoke<void>("agents_pause", { id }),
  agentResume: (id: string) => invoke<void>("agents_resume", { id }),
  agentCancel: (id: string) => invoke<void>("agents_cancel", { id }),
  agentRetry: (id: string) => invoke<void>("agents_retry", { id }),
  agentRaise: (id: string) => invoke<void>("agents_raise", { id }),
  agentFollowUp: (id: string, text: string) => invoke<void>("agents_follow_up", { id, text }),
  /** Tells a working agent something; a finished one takes it as a follow-up. */
  agentSteer: (id: string, text: string) => invoke<void>("agents_steer", { id, text }),
  agentUndo: (id: string) => invoke<string[]>("agents_undo", { id }),
  browserInfo: () => invoke<BrowserInfo>("agents_browser_info"),
  builders: () => invoke<CoderInfo[]>("agents_builders"),
  canSeeWindows: () => invoke<boolean>("privacy_can_see_windows"),
  permissions: () => invoke<Permissions>("permissions_status"),
  permissionsRequest: () => invoke<Permissions>("permissions_request"),
  permissionsOpen: (which: "screen" | "accessibility") => invoke<void>("permissions_open", { which }),
  /** Downloads Chromium for agents (about 150 MB); resolves to its path. */
  browserDownload: () => invoke<string>("agents_browser_download"),
  /** The first rows of a CSV the agent saved. */
  csvPreview: (id: string, path: string) => invoke<string[][]>("agents_csv_preview", { id, path }),
  /** Opens a file the agent made in the app that handles it. */
  openAgentFile: (id: string, path: string) => invoke<void>("agents_open_file", { id, path }),
  agentRename: (id: string, name: string) => invoke<void>("agents_rename", { id, name }),
  agentDismiss: (id: string) => invoke<void>("agents_dismiss", { id }),
  agentSeen: (id: string) => invoke<void>("agents_seen", { id }),
  agentDelete: (id: string) => invoke<void>("agents_delete", { id }),
  agentDuplicate: (id: string) => invoke<void>("agents_duplicate", { id }),
  agentExport: (id: string, path: string) => invoke<void>("agents_export", { id, path }),
  setBraveKey: (key: string) => invoke<void>("agents_set_brave_key", { key }),
  hasBraveKey: () => invoke<boolean>("agents_has_brave_key"),
  plan: (request: string) => invoke<Plan>("agents_plan", { request }),
  templates: () => invoke<TemplateInfo[]>("agents_templates"),
  /** Fills in a template and shows its plan card. */
  templatePlan: (id: string, values: Record<string, string>) => invoke<Plan>("agents_template_plan", { id, values }),
  /** Saves an agent's request (all its agents) as a template. */
  saveTemplate: (id: string) => invoke<Template>("agents_save_template", { id }),
  toolGroups: () => invoke<ToolGroup[]>("agents_tool_groups"),
  triggersStatus: () => invoke<TriggerStatus[]>("agents_triggers_status"),
  triggerRunNow: (id: string) => invoke<void>("agents_trigger_run_now", { id }),
  /** Clears a trigger's pause after failures. */
  triggerResume: (id: string) => invoke<void>("agents_trigger_resume", { id }),
  planCurrent: () => invoke<Plan | null>("agents_plan_current"),
  planStart: (id: string, mode: RunMode) => invoke<void>("agents_plan_start", { id, mode }),
  planCancel: () => invoke<void>("agents_plan_cancel"),
  /** Opens the agent panel, at one agent if given. */
  openAgentPanel: (id?: string) => invoke<void>("agents_open_panel", { id: id ?? null }),
  agentVoiceFollowUp: (id: string) => invoke<void>("agents_voice_follow_up", { id }),
  dockLayout: (width: number, height: number) => invoke<void>("dock_layout", { width, height }),
  dockFocus: (on: boolean) => invoke<void>("dock_focus", { on }),
  /** Opens an https page in the browser. */
  openLink: (url: string) => invoke<void>("open_link", { url }),
  connectors: () => invoke<ConnectorInfo[]>("connectors_list"),
  /** Signs in in the browser; resolves once the user is back. */
  connectorConnect: (id: string) => invoke<Connection>("connectors_connect", { id }),
  connectorSetToken: (id: string, token: string) => invoke<Connection>("connectors_set_token", { id, token }),
  connectorDisconnect: (id: string) => invoke<void>("connectors_disconnect", { id }),
  connectorTest: (id: string) => invoke<string>("connectors_test", { id }),
  /** An OAuth app's client secret, kept in the keychain. Empty removes it. */
  setAppSecret: (provider: string, secret: string) => invoke<void>("connectors_set_app_secret", { provider, secret }),
  mcpStatus: () => invoke<ServerStatus[]>("mcp_status"),
  mcpConnect: (id: string) => invoke<ToolInfo[]>("mcp_connect", { id }),
  mcpSignIn: (id: string) => invoke<ToolInfo[]>("mcp_sign_in", { id }),
  mcpSignOut: (id: string) => invoke<void>("mcp_sign_out", { id }),
  /** A secret variable ("env"), header ("header") or OAuth client secret ("oauth"). */
  mcpSetSecret: (id: string, kind: "env" | "header" | "oauth", key: string, value: string) => invoke<void>("mcp_set_secret", { id, kind, key, value }),
  mcpImport: (json: string) => invoke<ImportReport>("mcp_import", { json }),
  mcpExport: () => invoke<string>("mcp_export"),
  mcpCatalog: () => invoke<Preset[]>("mcp_catalog"),
};

export function asFieldErrors(e: unknown): FieldError[] {
  if (Array.isArray(e)) return e as FieldError[];
  return [{ path: "", message: String(e) }];
}
