import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { CursorFrame } from "../bindings/CursorFrame";
import { Buddy } from "../buddy/Buddy";
import { followStep, type Point } from "../buddy/follow";
import { api, EVENTS } from "../lib/ipc";
import { useSettings } from "../lib/useSettings";

/** The click-through layer on one monitor. Phase 1 draws only the buddy. */
export function Overlay() {
  const [settings] = useSettings();
  const [visible, setVisible] = useState(false);
  const [inside, setInside] = useState(false);
  const [customSrc, setCustomSrc] = useState<string | null>(null);
  const el = useRef<HTMLDivElement>(null);
  const target = useRef<Point | null>(null);
  const pos = useRef<Point | null>(null);
  const smoothness = useRef(0);
  smoothness.current = settings?.buddy.smoothness ?? 0;

  useEffect(() => {
    // Cursor frames are sent to this window only, so listen on the window.
    const offCursor = getCurrentWebviewWindow().listen<CursorFrame>(EVENTS.cursor, ({ payload }) => {
      setInside(payload.inside);
      if (!payload.inside) {
        // Re-enter from the new spot instead of gliding across the screen.
        target.current = pos.current = null;
        return;
      }
      target.current = { x: payload.x, y: payload.y };
      pos.current ??= target.current;
    });
    const offVisible = listen<boolean>(EVENTS.buddyVisible, (e) => setVisible(e.payload));
    api.overlayReady().then(setVisible);
    return () => {
      offCursor.then((f) => f());
      offVisible.then((f) => f());
    };
  }, []);

  const style = settings?.buddy.style;
  useEffect(() => {
    if (style !== "custom") return;
    const load = () => api.customBuddy().then(setCustomSrc);
    load();
    const off = listen(EVENTS.buddyImageChanged, load);
    return () => void off.then((f) => f());
  }, [style]);

  // Position is written straight to the DOM each frame; no React renders.
  useEffect(() => {
    let raf = 0;
    let last = performance.now();
    const tick = (now: number) => {
      const dt = Math.min(now - last, 100);
      last = now;
      if (target.current && pos.current && el.current) {
        pos.current = followStep(pos.current, target.current, smoothness.current, dt);
        el.current.style.transform = `translate3d(${pos.current.x}px, ${pos.current.y}px, 0)`;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  if (!settings) return null;
  const b = settings.buddy;
  const shown = visible && inside;

  return (
    <div ref={el} className="overlay-buddy" style={{ opacity: shown ? b.opacity : 0 }}>
      <div style={{ transform: `translate(${b.offsetX}px, ${b.offsetY}px)` }}>
        <Buddy
          style={b.style}
          size={b.size}
          state="idle"
          animate={b.showStateAnimations}
          customSrc={customSrc}
        />
      </div>
    </div>
  );
}
