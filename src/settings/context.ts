import { createContext, useContext } from "react";
import type { SettingsStore } from "../lib/useSettings";
import type { PlatformInfo } from "../bindings/PlatformInfo";

export interface Ctx {
  store: SettingsStore;
  platform: PlatformInfo;
  /** Hotkeys the OS refused, keyed by action. */
  hotkeyErrors: Record<string, string>;
  toast: (text: string, tone?: "ok" | "error") => void;
}

export const SettingsCtx = createContext<Ctx | null>(null);

export function useCtx(): Ctx {
  const c = useContext(SettingsCtx);
  if (!c) throw new Error("SettingsCtx missing");
  return c;
}
