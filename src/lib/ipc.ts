// Typed wrappers around Helpy's Rust commands and events.
import { invoke } from "@tauri-apps/api/core";
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
};

export function asFieldErrors(e: unknown): FieldError[] {
  if (Array.isArray(e)) return e as FieldError[];
  return [{ path: "", message: String(e) }];
}
