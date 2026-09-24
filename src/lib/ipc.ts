// Typed wrappers around Helpy's Rust commands and events.
import { Channel, invoke } from "@tauri-apps/api/core";
import type { AskEvent } from "../bindings/AskEvent";
import type { AskStatus } from "../bindings/AskStatus";
import type { LocalServer } from "../bindings/LocalServer";
import type { ModelInfo } from "../bindings/ModelInfo";
import type { ProviderConfig } from "../bindings/ProviderConfig";
import type { UsageToday } from "../bindings/UsageToday";
import type { FieldError } from "../bindings/FieldError";
import type { HotkeyStatus } from "../bindings/HotkeyStatus";
import type { PlatformInfo } from "../bindings/PlatformInfo";
import type { Settings } from "../bindings/Settings";

export const EVENTS = {
  settingsChanged: "settings://changed",
  hotkeyStatus: "hotkeys://status",
  cursor: "overlay://cursor",
  buddyVisible: "buddy://visible",
  buddyImageChanged: "buddy://image-changed",
  openSection: "settings://open-section",
  askShown: "ask://shown",
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

  // The ask panel.
  ask: (text: string, onEvent: (e: AskEvent) => void) => {
    const channel = new Channel<AskEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("ask_send", { text, channel });
  },
  askCancel: () => invoke<void>("ask_cancel"),
  askReset: () => invoke<void>("ask_reset"),
  askScreenAnswer: (id: number, allow: boolean) => invoke<void>("ask_screen_answer", { id, allow }),
  askStatus: () => invoke<AskStatus>("ask_status"),
  askHide: () => invoke<void>("ask_hide"),
  openSettingsSection: (section: string) => invoke<void>("open_settings_section", { section }),
};

export function asFieldErrors(e: unknown): FieldError[] {
  if (Array.isArray(e)) return e as FieldError[];
  return [{ path: "", message: String(e) }];
}
