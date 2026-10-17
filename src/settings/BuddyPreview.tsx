import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { Buddy as BuddySettings } from "../bindings/Buddy";
import type { BuddyHome } from "../bindings/BuddyHome";
import type { BuddyStyle } from "../bindings/BuddyStyle";
import type { Hotkeys } from "../bindings/Hotkeys";
import { Buddy, type BuddyState } from "../buddy/Buddy";
import { followStep, type Point } from "../buddy/follow";
import { IslandPanel, PillFace, keys, type AgentRow, type Expression } from "../island/Island";
import { api, EVENTS } from "../lib/ipc";
import "../island/island.css";

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

/** Where the buddy lives: beside the cursor, or in the notch. Each choice
 *  shows a little picture of itself. */
export function HomePicker({ value, onChange }: { value: BuddyHome; onChange: (v: BuddyHome) => void }) {
  return (
    <div className="homes" role="radiogroup" aria-label="Where the buddy lives">
      <button type="button" role="radio" aria-checked={value === "cursor"} className="home" onClick={() => onChange("cursor")}>
        <span className="home__art home__art--cursor" aria-hidden="true">
          <PointerGlyph />
          <Buddy style="pip" size={24} animate={false} />
        </span>
        <span className="home__text">
          <strong>Beside the cursor</strong>
          <span>Rides along with your pointer on every screen, and shows what Helpy is doing right there.</span>
        </span>
      </button>
      <button type="button" role="radio" aria-checked={value === "notch"} className="home" onClick={() => onChange("notch")}>
        <span className="home__art home__art--notch" aria-hidden="true">
          <i className="home__bar" />
          <i className="home__notch" />
          <span className="home__strip">
            <PillFace expr="happy" />
          </span>
        </span>
        <span className="home__text">
          <strong>In the notch</strong>
          <span>Hidden until you hover it or it has something to show: it listens, answers and tells you what its agents are doing.</span>
        </span>
      </button>
    </div>
  );
}

type Scene = "idle" | "working" | "listening" | "thinking" | "approval" | "done";
const SCENES: { id: Scene; label: string }[] = [
  { id: "idle", label: "Idle" },
  { id: "working", label: "Agents working" },
  { id: "listening", label: "Listening" },
  { id: "thinking", label: "Thinking" },
  { id: "approval", label: "Needs you" },
  { id: "done", label: "All done" },
];
const row = (id: string, name: string, what: string, tone: string, steps: string | null): AgentRow => ({ id, name, what, tone, steps, goal: "" });
const LAPTOPS = row("a1", "Laptops", "Reading three reviews", "work", "2/5");
const INBOX = row("a2", "Inbox", "Sorting 38 emails", "work", "1/3");
const REPORT = row("a3", "Report", "Done", "done", null);
const EXPENSES = row("a4", "Expenses", "Needs your OK", "question", "2/4");
type SceneView = { expr: Expression; mood: BuddyState; title: string; line: string; rows: AgentRow[]; more?: number; working: number; busy: boolean; cheer?: boolean };
const VIEWS: Record<Scene, SceneView> = {
  idle: { expr: "happy", mood: "idle", title: "Hi there", line: "Ask me anything", rows: [], working: 0, busy: false },
  working: { expr: "working", mood: "idle", title: "2 agents working", line: "Laptops · Reading three reviews", rows: [LAPTOPS, INBOX, REPORT], working: 2, busy: false },
  listening: { expr: "listening", mood: "listening", title: "Listening", line: "", rows: [], working: 0, busy: true },
  thinking: { expr: "thinking", mood: "thinking", title: "“Which laptop is best for video editing?”", line: "Thinking…", rows: [], working: 0, busy: true },
  approval: { expr: "working", mood: "idle", title: "1 agent needs you", line: "Laptops · Reading three reviews", rows: [EXPENSES, LAPTOPS], more: 1, working: 3, busy: false },
  done: { expr: "excited", mood: "idle", title: "All done!", line: "Report finished", rows: [REPORT], working: 0, busy: false, cheer: true },
};
/** A MacBook 14" notch, in points. */
const NOTCH = { width: 185, height: 32 };

/**
 * A stage with a Mac menu bar and its notch, and the island under it with
 * your current settings, in a few example scenes. Hovering the notch opens
 * it, as on the real screen.
 */
export function NotchStage({ buddy, hotkeys }: { buddy: BuddySettings; hotkeys: Hotkeys }) {
  const [scene, setScene] = useState<Scene>("idle");
  const [hover, setHover] = useState(false);
  const [level, setLevel] = useState(0.4);
  useEffect(() => {
    if (scene !== "listening") return;
    const t = setInterval(() => setLevel(0.15 + Math.random() * 0.75), 120);
    return () => clearInterval(t);
  }, [scene]);
  const v = VIEWS[scene];
  const open = hover || v.busy || !!v.cheer;
  const atNotch = buddy.notchEdge === "top";
  const hidden = atNotch && !open && v.working === 0;
  return (
    <div className="stage-wrap">
      <div className={`stage stage--notch${open ? " is-hovering" : ""}`} onMouseLeave={() => setHover(false)}>
        <div className="menubar" aria-hidden="true">
          <span className="menubar__apple"></span>
          <span className="menubar__app">Finder</span>
          <span>File</span>
          <span>Edit</span>
          <span>View</span>
          <span>Go</span>
          <span>Window</span>
          <span>Help</span>
          <span className="menubar__right">Sat 9:41</span>
        </div>
        <div className={`stage__island stage__island--${buddy.notchEdge}`} onMouseEnter={() => setHover(true)}>
          <IslandPanel
            preview
            buddy={buddy}
            notch={atNotch ? NOTCH : null}
            open={open}
            hidden={hidden}
            busy={v.busy}
            sleepy={false}
            expr={v.expr}
            mood={v.mood}
            cheer={!!v.cheer}
            hover={hover}
            title={v.title}
            line={v.line}
            level={level}
            working={v.working}
            queued={0}
            rows={v.rows}
            more={v.more ?? 0}
            tips={{ voice: keys(hotkeys.voiceAsk), text: keys(hotkeys.textAsk), circle: keys(hotkeys.circleToExplain), usage: "Today · 14 calls · 8.3¢" }}
            spawn={0}
            dockSide="right"
            onSpawnEnd={() => {}}
            onAsk={() => {}}
            onAgents={() => {}}
            onSettings={() => {}}
          />
        </div>
        {atNotch && <i className="menubar__notch" aria-hidden="true" />}
        <span className="stage__hint">
          {!buddy.enabled ? "Off: turn on the buddy below" : hidden ? "Hidden in the notch. Move your mouse onto the notch" : open ? "Open. It closes when you move away" : "Agents are working, so the face peeks out"}
        </span>
      </div>
      <div className="stage__states" role="radiogroup" aria-label="Preview scene">
        {SCENES.map((s) => (
          <button key={s.id} type="button" role="radio" aria-checked={scene === s.id} className="chip" onClick={() => setScene(s.id)}>
            {s.label}
          </button>
        ))}
        <span className="stage__note">Example agents and your shortcuts</span>
      </div>
    </div>
  );
}

/** A quiet sketch of an ordinary app window, so the buddy has something to sit on. */
export function MockApp() {
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
