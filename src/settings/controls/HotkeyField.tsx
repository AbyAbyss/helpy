import { useEffect, useRef, useState } from "react";
import type { HotkeyAction } from "../../bindings/HotkeyAction";
import { api } from "../../lib/ipc";
import { acceleratorFromEvent, hasModifier, isFunctionKey, keycaps, reservedReason, type OS } from "../../lib/hotkeys";
import { useCtx } from "../context";

export function Keycaps({ accelerator, os }: { accelerator: string; os: OS }) {
  return (
    <span className="keycaps">
      {keycaps(accelerator, os).map((k, i) => (
        <kbd key={i}>{k}</kbd>
      ))}
    </span>
  );
}

/**
 * Click to record, press a combination, done. Escape cancels, Backspace clears.
 * Before saving, the combination is test-registered so a clash with another app
 * shows up here instead of silently failing later.
 */
export function HotkeyField({ action }: { action: HotkeyAction }) {
  const { store, platform, hotkeyErrors } = useCtx();
  const os = platform.os as OS;
  const path = `hotkeys.${action}`;
  const value = store.get<string>(path);
  const [recording, setRecording] = useState(false);
  const [live, setLive] = useState<string>("");
  const [localError, setLocalError] = useState<string | null>(null);
  const ref = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!recording) return;
    api.suspendHotkeys(true);
    const onKey = async (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) {
        setRecording(false);
        return;
      }
      if ((e.key === "Backspace" || e.key === "Delete") && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) {
        setRecording(false);
        setLocalError(null);
        store.set(path, "");
        return;
      }
      const mods = [e.ctrlKey && "Control", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && (os === "macos" ? "Command" : "Super")]
        .filter(Boolean)
        .join("+");
      const acc = acceleratorFromEvent(e, os);
      if (!acc) {
        setLive(mods);
        return;
      }
      if (!hasModifier(acc) && !isFunctionKey(acc)) {
        setLive(acc);
        setLocalError("Add Ctrl, Alt, Shift, or a system key so normal typing isn't captured.");
        return;
      }
      setRecording(false);
      setLive("");
      if (acc === value) {
        setLocalError(null);
        return;
      }
      const free = await api.probeHotkey(acc).catch(() => true);
      if (!free) {
        setLocalError("Another app is already using this combination. Pick a different one.");
        return;
      }
      setLocalError(null);
      store.set(path, acc);
    };
    const onUp = (e: KeyboardEvent) => {
      if (!e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) setLive("");
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("keyup", onUp, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("keyup", onUp, true);
      api.suspendHotkeys(false);
    };
  }, [recording, os, path, store, value]);

  const error = localError ?? store.errors[path] ?? hotkeyErrors[action] ?? null;
  const warning = !error ? reservedReason(value, os) : null;

  return (
    <div className="ctl-hotkey-wrap">
      <button
        ref={ref}
        type="button"
        className="ctl-hotkey"
        data-recording={recording ? "yes" : "no"}
        data-invalid={error ? "yes" : "no"}
        onClick={() => {
          setLocalError(null);
          setLive("");
          setRecording((r) => !r);
        }}
        onBlur={() => setRecording(false)}
        aria-label={recording ? "Recording, press a key combination" : `Change hotkey, currently ${value || "none"}`}
      >
        {recording ? (
          live ? (
            <Keycaps accelerator={live} os={os} />
          ) : (
            <span className="ctl-hotkey-prompt">Press keys…</span>
          )
        ) : value ? (
          <Keycaps accelerator={value} os={os} />
        ) : (
          <span className="ctl-hotkey-none">Not set</span>
        )}
      </button>
      {recording && <p className="row-hint">Esc cancels · Backspace removes the hotkey</p>}
      {error && <p className="row-error">{error}</p>}
      {warning && <p className="row-warn">Your system already uses this: it {warning}. Helpy will take it over while running.</p>}
    </div>
  );
}
