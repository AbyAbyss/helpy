// A small fake desktop where the buddy follows a scripted pointer, using the same
// physics and settings as the real overlay. Hover the stage to drive it yourself.

import { useEffect, useRef, useState } from "react";
import type { BuddyActivity } from "../../bindings/BuddyActivity";
import type { BuddyStyle } from "../../bindings/BuddyStyle";
import { Buddy } from "../../buddy/Buddy";
import { placement, step, type Vec } from "../../buddy/follow";
import { useCtx } from "../context";
import { useCustomBuddySrc } from "./extras";

const STATES: { value: BuddyActivity | "agents" | "attention"; label: string }[] = [
  { value: "idle", label: "Idle" },
  { value: "listening", label: "Listening" },
  { value: "thinking", label: "Thinking" },
  { value: "speaking", label: "Speaking" },
  { value: "agents", label: "3 agents" },
  { value: "attention", label: "Needs you" },
];

const FOLDERS = [
  ["Inbox", "12"],
  ["Drafts", "2"],
  ["Sent", ""],
  ["Junk Email", "4"],
  ["Archive", ""],
];

export function BuddyPreview() {
  const { store } = useCtx();
  const b = store.settings!.buddy;
  const customSrc = useCustomBuddySrc();
  const [state, setState] = useState<(typeof STATES)[number]["value"]>("idle");
  const [flipped, setFlipped] = useState(false);
  const stage = useRef<HTMLDivElement>(null);
  const buddy = useRef<HTMLDivElement>(null);
  const pointer = useRef<HTMLDivElement>(null);
  const user = useRef<Vec | null>(null);
  const cfg = useRef(b);
  cfg.current = b;

  useEffect(() => {
    let raf = 0;
    let last = performance.now();
    let pos: Vec | null = null;
    let wasFlipped = false;
    const t0 = performance.now();
    const tick = (now: number) => {
      const el = stage.current;
      if (el) {
        const w = el.clientWidth;
        const h = el.clientHeight;
        const t = (now - t0) / 1000;
        // A slow figure-eight that visits the edges so flipping is visible.
        const scripted = { x: w * (0.5 + 0.4 * Math.sin(t * 0.55)), y: h * (0.5 + 0.32 * Math.sin(t * 1.1 + 0.6)) };
        const target = user.current ?? scripted;
        const s = cfg.current;
        const want = placement(target, { x: s.offsetX, y: s.offsetY }, s.size, { width: w, height: h });
        pos = pos ? step(pos, want, (now - last) / 1000, s.smoothness) : { x: want.x, y: want.y };
        if (buddy.current) buddy.current.style.transform = `translate3d(${pos.x}px, ${pos.y}px, 0)`;
        if (pointer.current) {
          pointer.current.style.transform = `translate3d(${target.x}px, ${target.y}px, 0)`;
          pointer.current.style.opacity = user.current ? "0" : "1";
        }
        if (want.flipped !== wasFlipped) {
          wasFlipped = want.flipped;
          setFlipped(want.flipped);
        }
      }
      last = now;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  const activity: BuddyActivity = state === "agents" || state === "attention" ? "idle" : state;
  const style: BuddyStyle = b.style === "custom" && !customSrc ? "pip" : b.style;

  return (
    <figure className="preview">
      <div
        ref={stage}
        className="preview-stage"
        data-off={b.enabled ? "no" : "yes"}
        onPointerMove={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          user.current = { x: e.clientX - r.left, y: e.clientY - r.top };
        }}
        onPointerLeave={() => (user.current = null)}
      >
        <div className="preview-app" aria-hidden="true">
          <div className="preview-app-bar">
            <i />
            <i />
            <i />
            <span>Mail</span>
          </div>
          <div className="preview-app-body">
            <ul className="preview-folders">
              {FOLDERS.map(([name, n]) => (
                <li key={name} data-target={name === "Junk Email" ? "yes" : "no"}>
                  <span>{name}</span>
                  {n && <b>{n}</b>}
                </li>
              ))}
            </ul>
            <div className="preview-list">
              {[72, 54, 80, 46, 64].map((w, i) => (
                <div key={i} className="preview-msg">
                  <span style={{ width: `${w * 0.45}%` }} />
                  <span style={{ width: `${w}%` }} />
                </div>
              ))}
            </div>
          </div>
          <div className="preview-callout">
            <span>Junk Email is here</span>
          </div>
        </div>

        <div ref={pointer} className="preview-pointer" aria-hidden="true">
          <svg viewBox="0 0 16 22" width="16" height="22">
            <path d="M1 1v17l4.5-4.2 3 7 3-1.3-3-6.8H15Z" fill="#fff" stroke="#111" strokeWidth="1.3" strokeLinejoin="round" />
          </svg>
        </div>
        <div ref={buddy} className="preview-buddy" style={{ opacity: b.enabled ? b.opacity : 0 }}>
          <Buddy
            style={style}
            customSrc={customSrc}
            size={b.size}
            activity={activity}
            animate={b.showStateAnimations}
            agents={state === "agents" ? 3 : 0}
            showBadge={b.showAgentBadge}
            attention={state === "attention"}
            flipped={flipped}
          />
        </div>
        {!b.enabled && <p className="preview-off">The buddy is turned off</p>}
      </div>
      <figcaption className="preview-bar">
        <span className="preview-hint">Live preview. Move your pointer over it to take control.</span>
        <div className="chips" role="radiogroup" aria-label="Preview state">
          {STATES.map((s) => (
            <button key={s.value} type="button" role="radio" aria-checked={state === s.value} className="chip" onClick={() => setState(s.value)}>
              {s.label}
            </button>
          ))}
        </div>
      </figcaption>
    </figure>
  );
}
