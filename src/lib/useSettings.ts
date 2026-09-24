// Settings store for React. Changes apply optimistically, are batched for a moment
// (so dragging a slider doesn't write the file 60 times a second), and roll back per
// field with an inline error if the backend rejects them.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Settings } from "../bindings/Settings";
import { api, asCommandError, EVENTS, listen } from "./ipc";

export type Path = string; // "buddy.size"

export function getPath(obj: unknown, path: Path): unknown {
  return path.split(".").reduce<unknown>((o, k) => (o as Record<string, unknown> | undefined)?.[k], obj);
}

function setPath<T>(obj: T, path: Path, value: unknown): T {
  const copy = structuredClone(obj) as Record<string, unknown>;
  const keys = path.split(".");
  let cur = copy;
  for (const k of keys.slice(0, -1)) cur = cur[k] as Record<string, unknown>;
  cur[keys[keys.length - 1]] = value;
  return copy as T;
}

function toPatch(path: Path, value: unknown): Record<string, unknown> {
  return path
    .split(".")
    .reverse()
    .reduce<unknown>((acc, k) => ({ [k]: acc }), value) as Record<string, unknown>;
}

function mergePatch(a: Record<string, unknown>, b: Record<string, unknown>) {
  for (const [k, v] of Object.entries(b)) {
    if (v && typeof v === "object" && !Array.isArray(v) && a[k] && typeof a[k] === "object") {
      mergePatch(a[k] as Record<string, unknown>, v as Record<string, unknown>);
    } else a[k] = v;
  }
  return a;
}

export interface SettingsStore {
  settings: Settings | null;
  errors: Record<Path, string>;
  get: <T = unknown>(path: Path) => T;
  set: (path: Path, value: unknown) => void;
  replace: (s: Settings) => void;
  setErrors: (e: Record<Path, string>) => void;
  lastError: string | null;
}

export function useSettings(): SettingsStore {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [errors, setErrors] = useState<Record<Path, string>>({});
  const [lastError, setLastError] = useState<string | null>(null);
  const pending = useRef<{ patch: Record<string, unknown>; paths: Path[] } | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const confirmed = useRef<Settings | null>(null);

  useEffect(() => {
    api.getSettings().then((s) => {
      confirmed.current = s;
      setSettings(s);
    });
    const un = listen<Settings>(EVENTS.settingsChanged, (s) => {
      confirmed.current = s;
      // Keep local edits that haven't been sent yet.
      if (!pending.current) setSettings(s);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const flush = useCallback(async () => {
    const batch = pending.current;
    pending.current = null;
    if (!batch) return;
    try {
      const s = await api.updateSettings(batch.patch);
      confirmed.current = s;
      setSettings(s);
      setErrors((e) => {
        const next = { ...e };
        batch.paths.forEach((p) => delete next[p]);
        return next;
      });
      setLastError(null);
    } catch (raw) {
      const err = asCommandError(raw);
      if (err.kind === "invalid") {
        setErrors((e) => {
          const next = { ...e };
          batch.paths.forEach((p) => delete next[p]);
          err.errors.forEach((fe) => (next[fe.path] = fe.message));
          return next;
        });
      } else setLastError(err.message);
      // Roll back to what the backend holds.
      if (confirmed.current) setSettings(confirmed.current);
    }
  }, []);

  const set = useCallback(
    (path: Path, value: unknown) => {
      setSettings((s) => (s ? setPath(s, path, value) : s));
      const p = pending.current ?? { patch: {}, paths: [] };
      mergePatch(p.patch, toPatch(path, value));
      if (!p.paths.includes(path)) p.paths.push(path);
      pending.current = p;
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(flush, 90);
    },
    [flush],
  );

  const get = useCallback(<T,>(path: Path) => getPath(settings, path) as T, [settings]);

  const replace = useCallback((s: Settings) => {
    confirmed.current = s;
    setSettings(s);
    setErrors({});
  }, []);

  return useMemo(
    () => ({ settings, errors, get, set, replace, setErrors, lastError }),
    [settings, errors, get, set, replace, lastError],
  );
}
