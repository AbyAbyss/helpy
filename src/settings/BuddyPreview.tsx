import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { Buddy as BuddySettings } from "../bindings/Buddy";
import type { BuddyStyle } from "../bindings/BuddyStyle";
import { Buddy, type BuddyState } from "../buddy/Buddy";
import { followStep, type Point } from "../buddy/follow";
import { api, EVENTS } from "../lib/ipc";

export function useCustomBuddy() {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    const load = () => api.customBuddy().then(setSrc);
    load();
    const off = listen(EVENTS.buddyImageChanged, load);
    return () => void off.then((f) => f());
  }, []);
  return src;
}

const STATES: { id: BuddyState; label: string }[] = [
  { id: "idle", label: "Idle" },
  { id: "listening", label: "Listening" },
  { id: "thinking", label: "Thinking" },
  { id: "speaking", label: "Speaking" },
];

/**
 * A small stage where the buddy follows your real mouse with your current
 * settings. At rest it sits beside a drawn pointer in the middle.
 */
export function BuddyStage({ buddy, customSrc }: { buddy: BuddySettings; customSrc: string | null }) {
  const stage = useRef<HTMLDivElement>(null);
  const mover = useRef<HTMLDivElement>(null);
  const [state, setState] = useState<BuddyState>("idle");
  const [hovering, setHovering] = useState(false);
  const target = useRef<Point | null>(null);
  const pos = useRef<Point | null>(null);
  const smoothness = useRef(buddy.smoothness);
  smoothness.current = buddy.smoothness;

  useEffect(() => {
    let raf = 0;
    let last = performance.now();
    const tick = (now: number) => {
      const dt = Math.min(now - last, 100);
      last = now;
      const el = stage.current;
      if (el && mover.current) {
        const rest = { x: el.clientWidth / 2 - 8, y: el.clientHeight / 2 - 14 };
        const goal = target.current ?? rest;
        pos.current = followStep(pos.current ?? goal, goal, smoothness.current, dt);
        mover.current.style.transform = `translate3d(${pos.current.x}px, ${pos.current.y}px, 0)`;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <div className="stage-wrap">
      <div
        ref={stage}
        className={`stage${hovering ? " is-hovering" : ""}`}
        onPointerMove={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          target.current = { x: e.clientX - r.left, y: e.clientY - r.top };
        }}
        onPointerEnter={() => setHovering(true)}
        onPointerLeave={() => {
          target.current = null;
          setHovering(false);
        }}
      >
        <MockApp />
        <div ref={mover} className="stage__mover">
          {!hovering && <PointerGlyph />}
          <div className="stage__buddy" style={{ transform: `translate(${buddy.offsetX}px, ${buddy.offsetY}px)`, opacity: buddy.enabled ? buddy.opacity : 0.25 }}>
            <Buddy
              style={buddy.style}
              size={buddy.size}
              state={state}
              animate={buddy.showStateAnimations}
              badge={buddy.showAgentBadge ? 2 : null}
              customSrc={customSrc}
            />
          </div>
        </div>
        <span className="stage__hint">{hovering ? "This is how it follows your pointer" : "Move your mouse here"}</span>
      </div>
      <div className="stage__states" role="radiogroup" aria-label="Preview state">
        {STATES.map((s) => (
          <button key={s.id} type="button" role="radio" aria-checked={state === s.id} className="chip" onClick={() => setState(s.id)}>
            {s.label}
          </button>
        ))}
        <span className="stage__note">{buddy.showAgentBadge ? "Badge shows 2 example agents" : ""}</span>
      </div>
    </div>
  );
}

/** A quiet sketch of an ordinary app window, so the buddy has something to sit on. */
function MockApp() {
  return (
    <div className="mock" aria-hidden="true">
      <div className="mock__bar">
        <i /> <i /> <i />
        <span className="mock__title">Inbox</span>
      </div>
      <div className="mock__body">
        <div className="mock__nav">
          <b className="is-on">Inbox</b>
          <b>Drafts</b>
          <b>Sent</b>
          <b>Junk Email</b>
        </div>
        <div className="mock__list">
          {[78, 64, 88, 52, 70].map((w, i) => (
            <div key={i} className="mock__row">
              <span style={{ width: `${w * 0.4}%` }} />
              <span style={{ width: `${w}%` }} />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function PointerGlyph() {
  return (
    <svg className="stage__pointer" width="16" height="22" viewBox="0 0 16 22" aria-hidden="true">
      <path d="M1 1 L1 17 L5.2 13.2 L8 20 L10.8 18.8 L8.1 12.3 L14 12.3 Z" />
    </svg>
  );
}

const STYLES: { id: Exclude<BuddyStyle, "custom">; name: string }[] = [
  { id: "pip", name: "Pip" },
  { id: "spark", name: "Spark" },
  { id: "dot", name: "Dot" },
];

export function BuddyStylePicker(props: {
  value: BuddyStyle;
  customSrc: string | null;
  onChange: (s: BuddyStyle) => void;
  onError: (message: string) => void;
}) {
  const { value, customSrc, onChange, onError } = props;
  const upload = async () => {
    const path = await open({ multiple: false, filters: [{ name: "Buddy image", extensions: ["svg", "png"] }] });
    if (typeof path !== "string") return;
    try {
      await api.setCustomBuddy(path);
    } catch (e) {
      onError(String(e));
    }
  };
  return (
    <div className="styles" role="radiogroup" aria-label="Buddy style">
      {STYLES.map((s) => (
        <button key={s.id} type="button" role="radio" aria-checked={value === s.id} className="styles__tile" onClick={() => onChange(s.id)}>
          <Buddy style={s.id} size={30} animate={false} />
          <span>{s.name}</span>
        </button>
      ))}
      <button
        type="button"
        role="radio"
        aria-checked={value === "custom"}
        className="styles__tile"
        onClick={() => (customSrc && value !== "custom" ? onChange("custom") : upload())}
        title={customSrc ? "Click again to replace the image" : "Upload an SVG or PNG"}
      >
        {customSrc ? (
          <Buddy style="custom" size={30} customSrc={customSrc} animate={false} />
        ) : (
          <svg className="styles__upload" viewBox="0 0 24 24" width="30" height="30" aria-hidden="true">
            <path d="M12 16V5m0 0-4 4m4-4 4 4M5 15v3a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-3" />
          </svg>
        )}
        <span>{customSrc ? (value === "custom" ? "Replace" : "Yours") : "Upload"}</span>
      </button>
    </div>
  );
}
