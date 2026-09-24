// Runs in each per-monitor overlay window. The window ignores the mouse entirely,
// so nothing here can steal a click; it only draws.

import { useEffect, useRef, useState } from "react";
import type { Settings } from "../bindings/Settings";
import type { RuntimeState } from "../bindings/RuntimeState";
import type { OverlayGeometry } from "../bindings/OverlayGeometry";
import type { CursorPoint } from "../bindings/CursorPoint";
import type { FullscreenState } from "../bindings/FullscreenState";
import { api, EVENTS, listen } from "../lib/ipc";
import { Buddy } from "../buddy/Buddy";
import { placement, step, type Vec } from "../buddy/follow";
import "./overlay.css";

export function OverlayApp() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [runtime, setRuntime] = useState<RuntimeState | null>(null);
  const [geo, setGeo] = useState<OverlayGeometry | null>(null);
  const [customSrc, setCustomSrc] = useState<string | null>(null);
  const [inside, setInside] = useState(false);
  const [idle, setIdle] = useState(false);
  const [fullscreen, setFullscreen] = useState<FullscreenState>({ active: false, monitor: null });
  const [flipped, setFlipped] = useState(false);
  const [notice, setNotice] = useState<{ text: string; id: number } | null>(null);
  const [check, setCheck] = useState(false);

  const buddyRef = useRef<HTMLDivElement>(null);
  const noticeRef = useRef<HTMLDivElement>(null);
  const target = useRef<Vec | null>(null);
  const pos = useRef<Vec | null>(null);
  const lastMove = useRef(performance.now());
  const cfg = useRef({ settings, geo });
  cfg.current = { settings, geo };

  // Initial state and subscriptions.
  useEffect(() => {
    api.getSettings().then(setSettings);
    api.runtime().then(setRuntime);
    api.overlayGeometry().then(setGeo);
    const subs = [
      listen<Settings>(EVENTS.settingsChanged, setSettings),
      listen<RuntimeState>(EVENTS.runtime, setRuntime),
      listen<OverlayGeometry[]>(EVENTS.geometry, () => api.overlayGeometry().then(setGeo)),
      listen<FullscreenState>(EVENTS.fullscreen, setFullscreen),
      listen<string>(EVENTS.notice, (text) => setNotice({ text, id: Date.now() })),
      listen(EVENTS.check, () => setCheck(true)),
      listen(EVENTS.clear, () => setCheck(false)),
      listen<CursorPoint>(EVENTS.cursor, (p) => {
        const g = cfg.current.geo;
        if (!g) return;
        const isInside = p.x >= g.x && p.y >= g.y && p.x < g.x + g.width && p.y < g.y + g.height;
        setInside(isInside);
        if (!isInside) return;
        const local = { x: (p.x - g.x) / g.scale, y: (p.y - g.y) / g.scale };
        // Arriving from another monitor: appear at the cursor instead of gliding across.
        if (!target.current) pos.current = null;
        target.current = local;
        lastMove.current = performance.now();
        setIdle(false);
      }),
    ];
    return () => subs.forEach((s) => s.then((f) => f()));
  }, []);

  useEffect(() => {
    if (!inside) target.current = null;
  }, [inside]);

  const customImage = settings?.buddy.customImage;
  useEffect(() => {
    if (settings?.buddy.style === "custom") api.getCustomBuddy().then(setCustomSrc);
  }, [settings?.buddy.style, customImage]);

  // Auto-clear the alignment check after a while.
  useEffect(() => {
    if (!check) return;
    const t = window.setTimeout(() => api.clearAnnotations(), 8000);
    return () => window.clearTimeout(t);
  }, [check]);

  useEffect(() => {
    if (!notice) return;
    const t = window.setTimeout(() => setNotice(null), 3600);
    return () => window.clearTimeout(t);
  }, [notice]);

  // Idle auto-hide.
  useEffect(() => {
    if (!settings?.buddy.hideWhenIdle) return;
    const ms = settings.buddy.idleSeconds * 1000;
    const t = window.setInterval(() => setIdle(performance.now() - lastMove.current > ms), 500);
    return () => window.clearInterval(t);
  }, [settings?.buddy.hideWhenIdle, settings?.buddy.idleSeconds]);

  // Animation loop: writes transforms directly, no React render per frame.
  useEffect(() => {
    let raf = 0;
    let last = performance.now();
    let wasFlipped = false;
    const tick = (now: number) => {
      const dt = (now - last) / 1000;
      last = now;
      const { settings: s, geo: g } = cfg.current;
      const t = target.current;
      if (s && g && t) {
        const b = s.buddy;
        const bounds = { width: g.width / g.scale, height: g.height / g.scale };
        const want = placement(t, { x: b.offsetX, y: b.offsetY }, b.size, bounds);
        pos.current = pos.current ? step(pos.current, want, dt, b.smoothness) : { x: want.x, y: want.y };
        const { x, y } = pos.current;
        if (buddyRef.current) buddyRef.current.style.transform = `translate3d(${x}px, ${y}px, 0)`;
        if (noticeRef.current) noticeRef.current.style.transform = `translate3d(${x + b.size + 10}px, ${y + b.size / 2}px, 0)`;
        if (want.flipped !== wasFlipped) {
          wasFlipped = want.flipped;
          setFlipped(want.flipped);
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  if (!settings || !geo) return null;
  const b = settings.buddy;
  const fsHere =
    fullscreen.active &&
    fullscreen.monitor !== null &&
    fullscreen.monitor[0] === geo.x &&
    fullscreen.monitor[1] === geo.y;
  const visible = b.enabled && inside && !(b.hideInFullscreen && fsHere) && !(b.hideWhenIdle && idle);

  return (
    <div className="ov">
      {check && <AlignmentCheck geo={geo} />}
      <div ref={buddyRef} className="ov-buddy" data-visible={visible ? "yes" : "no"} style={{ opacity: visible ? b.opacity : 0 }}>
        <Buddy
          style={b.style}
          size={b.size}
          activity={runtime?.activity ?? "idle"}
          animate={b.showStateAnimations}
          agents={runtime?.agentsRunning ?? 0}
          showBadge={b.showAgentBadge}
          attention={runtime?.attention ?? false}
          customSrc={customSrc}
          flipped={flipped}
        />
      </div>
      <div ref={noticeRef} className="ov-notice-anchor">
        {notice && inside && (
          <div key={notice.id} className="ov-notice">
            {notice.text}
          </div>
        )}
      </div>
    </div>
  );
}

function AlignmentCheck({ geo }: { geo: OverlayGeometry }) {
  return (
    <div className="ov-check">
      <div className="ov-check-label">
        <strong>{geo.name}</strong>
        <span>
          {geo.width} × {geo.height} px · {Math.round(geo.scale * 100)}% scale · origin {geo.x}, {geo.y}
        </span>
        <em>The teal frame should sit exactly on this screen's edges. Press Esc to clear.</em>
      </div>
      <span className="ov-check-corner tl" />
      <span className="ov-check-corner tr" />
      <span className="ov-check-corner bl" />
      <span className="ov-check-corner br" />
    </div>
  );
}
