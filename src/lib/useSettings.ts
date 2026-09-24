import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Settings } from "../bindings/Settings";
import { api, EVENTS } from "./ipc";

/** Live settings: loads once, then follows every change from any window. */
export function useSettings(): [Settings | null, (s: Settings) => void] {
  const [settings, setSettings] = useState<Settings | null>(null);
  useEffect(() => {
    api.getSettings().then(setSettings);
    const off = listen<Settings>(EVENTS.settingsChanged, (e) => setSettings(e.payload));
    return () => void off.then((f) => f());
  }, []);
  return [settings, setSettings];
}

/** Applies the theme setting to <html data-theme>; "system" follows the OS. */
export function useTheme(settings: Settings | null) {
  const theme = settings?.general.theme;
  useEffect(() => {
    const root = document.documentElement;
    if (!theme || theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
  }, [theme]);
}
