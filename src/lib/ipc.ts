// Typed wrappers around Tauri commands and events.
//
// When the UI runs in a plain browser (`npm run dev` without Tauri, or design previews),
// a small in-memory mock stands in for the backend so every screen still renders.

import type { Settings } from "../bindings/Settings";
import type { SectionId } from "../bindings/SectionId";
import type { RuntimeState } from "../bindings/RuntimeState";
import type { OverlayGeometry } from "../bindings/OverlayGeometry";
import type { PlatformInfo } from "../bindings/PlatformInfo";
import type { CommandError } from "../bindings/CommandError";
import type { FieldError } from "../bindings/FieldError";
import defaults from "../bindings/defaultSettings.json";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const EVENTS = {
  settingsChanged: "settings://changed",
  settingsNavigate: "settings://navigate",
  runtime: "runtime://changed",
  hotkeyStatus: "hotkeys://status",
  cursor: "cursor://move",
  geometry: "overlay://geometry",
  clear: "overlay://clear",
  check: "overlay://check",
  notice: "overlay://notice",
  fullscreen: "fullscreen://changed",
} as const;

type Unlisten = () => void;

export async function listen<T>(event: string, cb: (payload: T) => void): Promise<Unlisten> {
  if (!inTauri) {
    const h = (e: Event) => cb((e as CustomEvent<T>).detail);
    window.addEventListener(event, h);
    return () => window.removeEventListener(event, h);
  }
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => cb(e.payload));
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) return mock<T>(cmd, args);
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export function windowLabel(): string {
  if (!inTauri) return new URLSearchParams(location.search).get("window") ?? "settings";
  // Read synchronously from the injected metadata so routing needs no await.
  const meta = (window as unknown as { __TAURI_INTERNALS__: { metadata: { currentWindow: { label: string } } } })
    .__TAURI_INTERNALS__.metadata;
  return meta.currentWindow.label;
}

/** Turns whatever a command rejected with into a CommandError. */
export function asCommandError(e: unknown): CommandError {
  if (e && typeof e === "object" && "kind" in e) return e as CommandError;
  return { kind: "message", message: typeof e === "string" ? e : String(e) };
}

export const api = {
  getSettings: () => call<Settings>("get_settings"),
  updateSettings: (patch: unknown) => call<Settings>("update_settings", { patch }),
  resetSection: (section: SectionId) => call<Settings>("reset_settings_section", { section }),
  resetAll: () => call<Settings>("reset_all_settings"),
  exportSettings: (path: string) => call<void>("export_settings", { path }),
  importSettings: (path: string) => call<Settings>("import_settings", { path }),
  setCustomBuddy: (path: string) => call<Settings>("set_custom_buddy", { path }),
  getCustomBuddy: () => call<string | null>("get_custom_buddy"),
  runtime: () => call<RuntimeState>("get_runtime_state"),
  hotkeyStatus: () => call<Record<string, string>>("hotkey_status"),
  probeHotkey: (accelerator: string) => call<boolean>("probe_hotkey", { accelerator }),
  suspendHotkeys: (suspended: boolean) => call<void>("set_hotkeys_suspended", { suspended }),
  overlayGeometry: () => call<OverlayGeometry | null>("overlay_geometry"),
  listDisplays: () => call<OverlayGeometry[]>("list_displays"),
  showOverlayCheck: () => call<void>("show_overlay_check"),
  clearAnnotations: () => call<void>("clear_annotations"),
  platform: () => call<PlatformInfo>("platform_info"),
};

export async function pickFile(opts: { title: string; extensions: string[]; name: string; save?: boolean; defaultPath?: string }) {
  if (!inTauri) return null;
  const dialog = await import("@tauri-apps/plugin-dialog");
  const filters = [{ name: opts.name, extensions: opts.extensions }];
  if (opts.save) return dialog.save({ title: opts.title, filters, defaultPath: opts.defaultPath });
  const picked = await dialog.open({ title: opts.title, filters, multiple: false, directory: false });
  return typeof picked === "string" ? picked : null;
}

// ---------------------------------------------------------------- browser mock

let mockSettings: Settings = structuredClone(defaults) as Settings;

function mockEmit(event: string, detail: unknown) {
  window.dispatchEvent(new CustomEvent(event, { detail }));
}

function merge(base: Record<string, unknown>, patch: Record<string, unknown>) {
  for (const [k, v] of Object.entries(patch)) {
    if (v && typeof v === "object" && !Array.isArray(v) && base[k] && typeof base[k] === "object") {
      merge(base[k] as Record<string, unknown>, v as Record<string, unknown>);
    } else base[k] = v;
  }
}

/** Mirrors the few range checks the Rust validator does, so previews show inline errors too. */
function mockValidate(s: Settings): FieldError[] {
  const out: FieldError[] = [];
  const r = (path: string, v: number, min: number, max: number) => {
    if (v < min || v > max) out.push({ path, message: `Use a value from ${min} to ${max}.` });
  };
  r("buddy.size", s.buddy.size, 24, 96);
  r("buddy.idleSeconds", s.buddy.idleSeconds, 3, 600);
  r("buddy.offsetX", s.buddy.offsetX, -120, 120);
  r("buddy.offsetY", s.buddy.offsetY, -120, 120);
  const seen = new Map<string, string>();
  for (const [k, v] of Object.entries(s.hotkeys)) {
    if (k === "voiceMode" || !v) continue;
    const norm = String(v).split("+").map((p) => p.toLowerCase()).sort().join("+");
    if (seen.has(norm)) out.push({ path: `hotkeys.${k}`, message: `Already used by another Helpy hotkey.` });
    seen.set(norm, k);
  }
  return out;
}

async function mock<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const done = (s: Settings) => {
    mockSettings = s;
    mockEmit(EVENTS.settingsChanged, s);
    return structuredClone(s) as T;
  };
  switch (cmd) {
    case "get_settings":
      return structuredClone(mockSettings) as T;
    case "update_settings": {
      const next = structuredClone(mockSettings);
      merge(next as unknown as Record<string, unknown>, args!.patch as Record<string, unknown>);
      const errors = mockValidate(next);
      if (errors.length) throw { kind: "invalid", errors } satisfies CommandError;
      return done(next);
    }
    case "reset_settings_section": {
      const next = structuredClone(mockSettings) as unknown as Record<string, unknown>;
      const id = args!.section as string;
      next[id] = structuredClone((defaults as Record<string, unknown>)[id]);
      return done(next as unknown as Settings);
    }
    case "reset_all_settings":
      return done(structuredClone(defaults) as Settings);
    case "get_runtime_state":
      return { activity: "idle", capturePaused: false, agentsRunning: 0, attention: false, approvalNeeded: false, annotationsVisible: false } as T;
    case "hotkey_status":
      return {} as T;
    case "probe_hotkey":
      return true as T;
    case "platform_info": {
      const mac = /Mac/.test(navigator.platform);
      return { os: mac ? "macos" : /Win/.test(navigator.platform) ? "windows" : "linux", wayland: false, version: "0.1.0" } as T;
    }
    case "list_displays":
      return [
        { label: "overlay-0", index: 0, name: "Built-in Display", x: 0, y: 0, width: 2880, height: 1800, scale: 2, primary: true },
        { label: "overlay-1", index: 1, name: "DELL U2723QE", x: 2880, y: 0, width: 3840, height: 2160, scale: 1.5, primary: false },
      ] as T;
    case "get_custom_buddy":
    case "overlay_geometry":
      return null as T;
    default:
      return undefined as T;
  }
}
