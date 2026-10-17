import { useEffect, useRef, useState, type MouseEvent, type ReactNode, type Ref } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AgentView } from "../bindings/AgentView";
import type { AskActivity } from "../bindings/AskActivity";
import type { NotchInfo } from "../bindings/NotchInfo";
import type { Buddy as BuddySettings } from "../bindings/Buddy";
import type { Settings } from "../bindings/Settings";
import type { UsageToday } from "../bindings/UsageToday";
import type { VoicePhase } from "../bindings/VoicePhase";
import { Buddy, type BuddyState } from "../buddy/Buddy";
import { api, EVENTS } from "../lib/ipc";
import { useSettings } from "../lib/useSettings";
import { demo } from "./demo";

export type Expression = "happy" | "listening" | "thinking" | "speaking" | "working" | "excited" | "sleepy";

/**
 * The island as a character: white eyes with dark pupils, a small mouth and
 * two hands on the rim, drawn in a 60 by 30 box. Each expression changes
 * the eyes and mouth; the extras (a question mark, sparks, sound waves,
 * a progress bar, z's) sit around them.
 */
export function PillFace({ expr }: { expr: Expression }) {
  const closed = expr === "excited" || expr === "sleepy";
  const eye = (cx: number, squint: boolean) => (
    <g className="pf__eye" style={squint ? { transform: "scaleY(0.55)" } : undefined}>
      <ellipse cx={cx} cy={13} rx={expr === "listening" ? 7 : 6.2} ry={expr === "listening" ? 8.2 : 7.4} fill="#fff" />
      <ellipse className="pf__pupil" cx={cx + (expr === "thinking" ? 1.6 : 0)} cy={13 + (expr === "thinking" ? -1.8 : 0.6)} rx={expr === "listening" ? 3.8 : 3.3} ry={expr === "listening" ? 4.6 : 4.1} fill="#0b0d14" />
      <circle cx={cx - 1.4 + (expr === "thinking" ? 1.6 : 0)} cy={10.6 + (expr === "thinking" ? -1.8 : 0)} r={1.25} fill="#fff" />
    </g>
  );
  return (
    <svg className={`pf pf--${expr}`} viewBox="0 0 60 30" width="60" height="30" aria-hidden="true">
      {/* Hands on the rim. */}
      <g className="pf__hands">
        <circle cx={9} cy={27.5} r={4} fill="#1c2030" />
        <circle cx={51} cy={27.5} r={4} fill="#1c2030" />
        <circle cx={9} cy={26.5} r={3} fill="#262b3b" />
        <circle cx={51} cy={26.5} r={3} fill="#262b3b" />
      </g>
      {closed ? (
        <g className="pf__closed">
          {expr === "excited" ? (
            <>
              <path d="M12 15 q6 -8 12 0" fill="none" stroke="#fff" strokeWidth={2.6} strokeLinecap="round" />
              <path d="M36 15 q6 -8 12 0" fill="none" stroke="#fff" strokeWidth={2.6} strokeLinecap="round" />
            </>
          ) : (
            <>
              <path d="M12 12 q6 6 12 0" fill="none" stroke="#8fc6ff" strokeWidth={2.4} strokeLinecap="round" />
              <path d="M36 12 q6 6 12 0" fill="none" stroke="#8fc6ff" strokeWidth={2.4} strokeLinecap="round" />
            </>
          )}
        </g>
      ) : (
        <g className="pf__eyes">
          {eye(18, expr === "thinking")}
          {eye(42, false)}
        </g>
      )}
      {/* Brows: focused while working, one raised while thinking. */}
      {expr === "working" && (
        <>
          <path d="M11 4.5 l10 2.5" stroke="#fff" strokeWidth={1.8} strokeLinecap="round" />
          <path d="M49 4.5 l-10 2.5" stroke="#fff" strokeWidth={1.8} strokeLinecap="round" />
        </>
      )}
      {expr === "thinking" && <path d="M36 4 l11 -1.5" stroke="#8fc6ff" strokeWidth={1.8} strokeLinecap="round" />}
      {/* Mouth. */}
      {expr === "excited" ? (
        <ellipse className="pf__mouth" cx={30} cy={22.5} rx={4.2} ry={3.4} fill="#ff6f91" />
      ) : expr === "thinking" ? (
        <path d="M27 23 l6 -1.5" stroke="#8fc6ff" strokeWidth={1.8} strokeLinecap="round" />
      ) : expr === "sleepy" ? (
        <path d="M27.5 21.5 q2.5 2.4 5 0" fill="none" stroke="#8fc6ff" strokeWidth={1.6} strokeLinecap="round" />
      ) : expr === "working" ? (
        <g clipPath="url(#pf-track)">
          <rect x={14} y={22} width={32} height={3.6} rx={1.8} fill="#223058" />
          <rect className="pf__bar" x={14} y={22} width={14} height={3.6} rx={1.8} fill="url(#pf-bar)" />
        </g>
      ) : (
        <path className="pf__mouth" d="M26.5 20.5 q3.5 4.5 7 0 z" fill="#ff6f91" />
      )}
      {/* Extras. */}
      {expr === "thinking" && <text className="pf__q" x="52" y="9">?</text>}
      {expr === "sleepy" && (
        <g className="pf__zzz">
          <text x="50" y="10">z</text>
          <text x="54" y="6">z</text>
        </g>
      )}
      {expr === "excited" && (
        <g className="pf__sparks" stroke="#ffd166" strokeWidth={1.8} strokeLinecap="round">
          <path d="M3 7 l2 4" /><path d="M8 5 l0.5 4" /><path d="M57 7 l-2 4" /><path d="M52 5 l-0.5 4" />
        </g>
      )}
      {expr === "listening" && (
        <g className="pf__waves" fill="none" stroke="#7fb3ff" strokeWidth={1.6} strokeLinecap="round">
          <path d="M4 9 q-3 4 0 8" /><path d="M1 7 q-5 6 0 12" />
          <path d="M56 9 q3 4 0 8" /><path d="M59 7 q5 6 0 12" />
        </g>
      )}
      <defs>
        <clipPath id="pf-track">
          <rect x={14} y={22} width={32} height={3.6} rx={1.8} />
        </clipPath>
        <linearGradient id="pf-bar" x1="0" x2="1">
          <stop offset="0" stopColor="#2f6bff" /><stop offset="0.5" stopColor="#8fb6ff" /><stop offset="1" stopColor="#2f6bff" />
        </linearGradient>
      </defs>
    </svg>
  );
}

/** A finished agent, for the list. */
const OVER = ["done", "failed", "stopped", "cancelled", "ready"];

/** The side card's colour for an agent: blue working, yellow a question or
 *  approval, red failed, green done, grey otherwise. */
function tone(a: AgentView): string {
  if (a.pending?.type === "failure" || a.status === "failed") return "alert";
  if (a.pending || a.status === "approval" || a.status === "question") return "question";
  if (a.status === "running") return "work";
  if (a.status === "done") return "done";
  return "idle";
}

/** "Alt+Shift+Space" as the keys a Mac or PC shows: ⌥⇧Space or Alt+Shift+Space. */
export function keys(combo: string): string {
  if (!/Mac/i.test(navigator.platform)) return combo.replace(/\+/g, " ");
  const map: Record<string, string> = { Alt: "⌥", Option: "⌥", Shift: "⇧", Control: "⌃", Ctrl: "⌃", Super: "⌘", Command: "⌘", Cmd: "⌘", Meta: "⌘" };
  return combo.split("+").map((k) => map[k] ?? k).join("");
}

function money(n: number): string {
  return n >= 1 ? `$${n.toFixed(2)}` : `${(n * 100).toFixed(1)}¢`;
}

const IconChat = () => (
  <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true"><path d="M4 4.5h12a1.5 1.5 0 0 1 1.5 1.5v6a1.5 1.5 0 0 1-1.5 1.5H9l-3.6 2.8a.6.6 0 0 1-1-.5V13.5H4A1.5 1.5 0 0 1 2.5 12V6A1.5 1.5 0 0 1 4 4.5z" fill="currentColor" /></svg>
);
const IconAgents = () => (
  <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true" fill="currentColor"><circle cx="10" cy="4.5" r="2.5" /><circle cx="4.5" cy="14" r="2.5" /><circle cx="15.5" cy="14" r="2.5" /><path d="M10 7v3M8.3 10.8 5.8 12m5.9-1.2 2.5 1.2" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" fill="none" /></svg>
);
const IconGear = () => (
  <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true"><path d="M8.6 2.5h2.8l.5 2.1 1.5.8 2-.8 1.4 2.4-1.6 1.4v1.7l1.6 1.4-1.4 2.4-2-.8-1.5.8-.5 2.1H8.6l-.5-2.1-1.5-.8-2 .8-1.4-2.4 1.6-1.4V8.4L3.2 7l1.4-2.4 2 .8 1.5-.8z" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" /><circle cx="10" cy="10" r="2.4" fill="none" stroke="currentColor" strokeWidth="1.5" /></svg>
);

/** The voice level as a row of bars, each swinging by its own amount. */
const WEIGHTS = [0.35, 0.6, 0.85, 0.5, 1, 0.7, 0.45, 0.9, 0.55, 0.8, 0.4, 0.65];
function Waves({ level, live }: { level: number; live: boolean }) {
  return (
    <span className={`island__waves${live ? " is-live" : ""}`} aria-hidden="true">
      {WEIGHTS.map((w, i) => (
        <i key={i} style={{ height: `${live ? 4 + Math.min(1, level * 2.2) * w * 20 : 4}px`, animationDelay: `${i * 70}ms` }} />
      ))}
    </span>
  );
}

/**
 * The buddy's home at the screen edge: a black island that merges with a
 * MacBook's notch (its top band hides behind the notch; Rust keeps the
 * window at the status-bar level so it can sit against it). Idle, it hides
 * behind the notch; while agents work a strip with the face shows below
 * it. Hovered, or while Helpy listens, thinks or speaks, it grows out of
 * the notch into a panel: a row
 * of actions beside the notch, the face as the hero of one card, and what
 * its agents are doing in another. A click on it opens the text chat.
 */
/** One agent in the island's side card. */
export type AgentRow = { id: string; name: string; what: string; tone: string; steps: string | null; goal: string };

/** What the island shows about an agent. */
function rowFor(a: AgentView): AgentRow {
  const what =
    a.pending?.type === "approval" ? "Needs your OK" : a.pending?.type === "question" ? "Has a question" : a.pending?.type === "failure" ? "Ran into trouble" : a.status === "done" ? "Done" : a.status === "failed" ? "Failed" : a.status === "cancelled" ? "Cancelled" : a.status === "stopped" ? "Stopped" : a.status === "ready" ? "Ready to start" : a.status === "paused" ? "Paused" : a.status === "queued" ? "Waiting its turn" : a.statusLine || a.goal;
  const steps = a.plan.length > 0 && !OVER.includes(a.status) ? `${a.plan.filter((p) => p.status === "done").length}/${a.plan.length}` : null;
  return { id: a.id, name: a.name, what, tone: tone(a), steps, goal: a.goal };
}

export type IslandViewProps = {
  buddy: BuddySettings;
  /** The notch on this screen; nothing hides without one. */
  notch: NotchInfo | null;
  open: boolean;
  hidden: boolean;
  busy: boolean;
  sleepy: boolean;
  expr: Expression;
  mood: BuddyState;
  cheer: boolean;
  hover: boolean;
  title: string;
  line: string;
  /** The voice level, 0 to 1, while listening. */
  level: number;
  working: number;
  queued: number;
  /** The agents listed in the side card, and how many more there are. */
  rows: AgentRow[];
  more: number;
  tips: { voice: string; text: string; circle: string; usage: string };
  spawn: number;
  dockSide: "left" | "right";
  onSpawnEnd: () => void;
  /** Shown inside the settings page rather than its own window. */
  preview?: boolean;
  rootRef?: Ref<HTMLDivElement>;
  onHover?: () => void;
  onAsk: () => void;
  onAgents: (id?: string) => void;
  onSettings: () => void;
};

/**
 * The island as it looks: the black shell merging with the notch, the
 * strip with the face, or the open panel with its row of actions, the hero
 * card and the side card. The `Island` below feeds it from the app; the
 * settings page feeds it example scenes.
 */
export function IslandPanel(p: IslandViewProps) {
  const { buddy: b, notch, open, hidden, busy, sleepy, expr, mood, cheer, hover, working, rows, more, tips } = p;
  // The notch's size in the island's own (zoomed) pixels. Along other edges,
  // or without a notch, nothing hides and the top row is a normal row.
  const z = b.islandSize || 1;
  const atNotch = b.notchEdge === "top" && !!notch;
  const notchW = atNotch ? notch!.width / z : 0;
  const band = atNotch ? notch!.height / z : 0;
  const stop = (e: MouseEvent) => e.stopPropagation();
  const style = b.style === "custom" ? "pip" : b.style;
  const face: ReactNode = b.islandLook === "face" ? (
    <PillFace expr={expr} />
  ) : (
    <Buddy style={style} size={28} state={mood} mood={cheer ? "happy" : sleepy && !hover ? "sleepy" : null} animate={b.showStateAnimations} badge={b.showAgentBadge && working > 0 ? working : null} queued={p.queued} />
  );

  return (
    <div
      ref={p.rootRef}
      className={`island island--${b.notchEdge} island--${b.islandLook}${open ? " is-open" : ""}${busy ? " is-busy" : ""}${sleepy && !open ? " is-sleepy" : ""}${atNotch ? " is-notched" : ""}${hidden ? " is-hidden" : ""}${p.preview ? " island--preview" : ""}`}
      style={{ zoom: z, "--notch-w": `${notchW}px`, "--band": `${band}px` } as React.CSSProperties}
      role="button"
      aria-label="Helpy. Click to ask"
      onMouseEnter={p.onHover}
      onClick={p.onAsk}
    >
      <span className="island__ear island__ear--l" aria-hidden="true" />
      <span className="island__ear island__ear--r" aria-hidden="true" />
      <div className="island__shell">
        {open ? (
          <>
            <div className="island__row">
              <div className="island__actions">
                <button type="button" className="island__btn is-on" title="Ask Helpy" onClick={(e) => { stop(e); p.onAsk(); }}><IconChat /></button>
                <button type="button" className="island__btn" title="Agents" onClick={(e) => { stop(e); p.onAgents(); }}>
                  <IconAgents />
                  {working > 0 && <b className="island__count">{working}</b>}
                </button>
              </div>
              <div className="island__gap" />
              <div className="island__actions island__actions--end">
                <button type="button" className="island__btn" title="Settings" onClick={(e) => { stop(e); p.onSettings(); }}><IconGear /></button>
              </div>
            </div>
            <div className="island__body">
              <div className={`island__card island__hero island__hero--${expr}`}>
                <span className={`island__face${p.spawn > 0 ? " is-pushing" : ""}`} key={p.spawn}>
                  {face}
                  {p.spawn > 0 && (
                    <span className={`island__spawn island__spawn--${p.dockSide}`} onAnimationEnd={p.onSpawnEnd}>
                      <Buddy style={style} size={16} baby state="idle" animate={false} />
                    </span>
                  )}
                </span>
                <span className="island__title" title={p.title}>{p.title}</span>
                {mood === "listening" ? (
                  <Waves level={p.level} live />
                ) : mood === "speaking" ? (
                  <Waves level={0.5} live />
                ) : (
                  <span className={`island__line${mood === "thinking" ? " is-pulsing" : ""}`} title={p.line}>{p.line}</span>
                )}
              </div>
              <div className="island__card island__side">
                {rows.length > 0 ? (
                  <>
                    {rows.map((a) => (
                      <button key={a.id} type="button" className={`island__agent tone-${a.tone}`} onClick={(e) => { stop(e); p.onAgents(a.id); }} title={a.goal}>
                        <i className="island__dot" />
                        <span className="island__name">{a.name}</span>
                        <span className="island__what">{a.what}</span>
                        {a.steps && <span className="island__steps">{a.steps}</span>}
                      </button>
                    ))}
                    {more > 0 && (
                      <button type="button" className="island__more" onClick={(e) => { stop(e); p.onAgents(); }}>
                        {more} more…
                      </button>
                    )}
                  </>
                ) : (
                  <div className="island__tips">
                    <span className="island__tip"><kbd>{tips.voice}</kbd>Talk to Helpy</span>
                    <span className="island__tip"><kbd>{tips.text}</kbd>Type a question</span>
                    <span className="island__tip"><kbd>{tips.circle}</kbd>Circle to explain</span>
                    <span className="island__usage">{tips.usage}</span>
                  </div>
                )}
              </div>
            </div>
          </>
        ) : hidden ? null : (
          <div className="island__rest">
            {b.islandLook === "peek" ? (
              <span className="island__peek" aria-hidden="true">
                <Buddy style={style} size={22} baby state="idle" mood={sleepy ? "sleepy" : null} animate={b.showStateAnimations} />
                {working > 0 && <b className="island__dot island__dot--peek" />}
              </span>
            ) : (
              <span className="island__rest-face" aria-hidden="true">
                <PillFace expr={expr} />
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * The buddy's home at the screen edge: a black island that merges with a
 * MacBook's notch (its top band hides behind the notch; Rust keeps the
 * window at the status-bar level so it can sit against it). Idle, it hides
 * behind the notch; while agents work a strip with the face shows below
 * it. Hovered, or while Helpy listens, thinks or speaks, it grows out of
 * the notch into a panel: a row of actions beside the notch, the face as
 * the hero of one card, and what its agents are doing in another. A click
 * on it opens the text chat.
 */
export function Island() {
  const [live] = useSettings();
  const settings: Settings | null = demo?.settings ?? live;
  const [hover, setHover] = useState(false);
  const [notch, setNotch] = useState<NotchInfo | null>(demo ? demo.notch : null);
  const [voice, setVoice] = useState<VoicePhase>(demo?.voice ?? { phase: "idle", message: null });
  const [level, setLevel] = useState(0);
  const [partial, setPartial] = useState("");
  const [speaking, setSpeaking] = useState(demo?.speaking ?? false);
  const [activity, setActivity] = useState<AskActivity>({ running: demo?.asking ?? false, queued: 0 });
  const [usage, setUsage] = useState<UsageToday | null>(demo?.usage ?? null);
  // Agents by id, for how many work and what each is doing right now.
  const [agents, setAgents] = useState<Map<string, AgentView>>(() => new Map((demo?.agents ?? []).map((a) => [a.id, a])));
  const known = useRef<Map<string, AgentView>>(agents);
  // A step an agent just reported, shown for a moment; and which running
  // agent the hero line narrates.
  const [toast, setToast] = useState<string | null>(null);
  const [narrated, setNarrated] = useState(0);
  const lastToast = useRef(-Infinity);
  // A model that's taking longer than usual to answer.
  const [slow, setSlow] = useState(false);
  // A little agent pushed off toward the dock as agents start.
  const [spawn, setSpawn] = useState(0);
  // The last agent just finished: a moment of cheer.
  const [cheer, setCheer] = useState(demo?.cheer ?? false);
  // The mouse has rested a while: it dozes off.
  const [sleepy, setSleepy] = useState(demo?.sleepy ?? false);
  const wasWorking = useRef(0);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (demo) return;
    const offs = [
      listen<VoicePhase>(EVENTS.voiceState, (e) => {
        setVoice(e.payload);
        if (e.payload.phase !== "listening") setPartial("");
      }),
      listen<number>(EVENTS.voiceLevel, (e) => setLevel(e.payload)),
      listen<string>(EVENTS.voicePartial, (e) => setPartial(e.payload)),
      listen<boolean>(EVENTS.speaking, (e) => setSpeaking(e.payload)),
      listen<AskActivity>(EVENTS.askActivity, (e) => setActivity(e.payload)),
      listen<AgentView>(EVENTS.agentUpdate, ({ payload: a }) => {
        const was = known.current.get(a.id);
        known.current = new Map(known.current).set(a.id, a);
        setAgents(known.current);
        // A new step from a working agent: say it, but not more than once
        // every few seconds.
        if (a.status === "running" && a.statusLine && a.statusLine !== was?.statusLine && performance.now() - lastToast.current > 8000) {
          lastToast.current = performance.now();
          setToast(`${a.name} · ${a.statusLine}`);
        }
      }),
      listen<string>(EVENTS.agentRemoved, ({ payload: id }) => {
        const n = new Map(known.current);
        n.delete(id);
        known.current = n;
        setAgents(n);
      }),
      getCurrentWindow().listen(EVENTS.islandSpawn, () => setSpawn((n) => n + 1)),
      // Rust watches the cursor against the island's rectangle: the page's
      // own mouse-leave doesn't arrive in a window that never takes focus.
      getCurrentWindow().listen<boolean>(EVENTS.islandHover, (e) => setHover(e.payload)),
      listen<boolean>(EVENTS.sleepy, (e) => setSleepy(e.payload)),
    ];
    api.islandNotch().then(setNotch, () => {});
    api.askActivity().then(setActivity, () => {});
    api.agents().then((l) => {
      known.current = new Map(l.agents.map((a) => [a.id, a]));
      setAgents(known.current);
    }, () => {});
    return () => offs.forEach((o) => void o.then((f) => f()));
  }, []);
  const top = [...agents.values()].filter((a) => !a.parent && !a.dismissed);
  const running = top.filter((a) => a.status === "running");
  const working = top.filter((a) => !OVER.includes(a.status)).length;
  const waiting = top.filter((a) => a.pending).length;
  // The side card lists what needs you first, then what's working, then
  // what finished in the last while or hasn't been looked at; older
  // finished agents stay in the panel.
  const rank = (a: AgentView) => (a.pending ? 0 : OVER.includes(a.status) ? 2 : 1);
  const recent = Date.now() - 15 * 60_000;
  const listed = top
    .filter((a) => a.pending || !OVER.includes(a.status) || a.unseen || (a.finished ?? 0) > recent)
    .sort((x, y) => rank(x) - rank(y) || y.created - x.created);

  // The toast shows for a moment; the narration moves on every few seconds.
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 2800);
    return () => clearTimeout(t);
  }, [toast]);
  useEffect(() => {
    if (running.length < 2) return;
    const t = setInterval(() => setNarrated((n) => n + 1), 4000);
    return () => clearInterval(t);
  }, [running.length]);

  useEffect(() => {
    const finished = wasWorking.current > 0 && working === 0;
    wasWorking.current = working;
    if (!finished) return;
    setCheer(true);
    const t = setTimeout(() => setCheer(false), 2600);
    return () => clearTimeout(t);
  }, [working]);

  // The window follows the content's size, and moves when the edge or the
  // spot along it changes.
  const b = settings?.buddy;
  useEffect(() => {
    const el = root.current;
    if (!el || demo) return;
    const report = () => {
      const r = el.getBoundingClientRect();
      api.islandLayout(Math.ceil(r.width), Math.ceil(r.height)).catch(() => {});
    };
    const ro = new ResizeObserver(report);
    ro.observe(el);
    report();
    return () => ro.disconnect();
  }, [b?.home, b?.enabled, b?.notchEdge, b?.notchOffset, b?.islandSize]);

  // The voice phase stays "thinking" after a spoken answer (the pill shows
  // the answer), so the question's activity says when it's really thinking.
  const mood: BuddyState =
    voice.phase === "listening" ? "listening" : speaking ? "speaking" : voice.phase === "transcribing" || activity.running ? "thinking" : "idle";
  useEffect(() => {
    setSlow(false);
    if (mood !== "thinking") return;
    const t = setTimeout(() => setSlow(true), 5000);
    return () => clearTimeout(t);
  }, [mood]);

  const busy = mood !== "idle";
  const open = demo?.open || hover || busy || spawn > 0 || cheer || !!toast;
  // Today's spending shows on the idle card; it's fetched as the island opens.
  useEffect(() => {
    if (!open || demo) return;
    api.usageToday().then(setUsage, () => {});
  }, [open]);

  if (!b || !settings) return null;
  // What the hero line says about its agents: a step just reported, or the
  // working agents one at a time.
  const now = running[narrated % Math.max(running.length, 1)];
  const agentLine = toast ?? (now?.statusLine ? `${now.name} · ${now.statusLine}` : working > 0 ? `${working} ${working === 1 ? "agent" : "agents"} working` : null);
  const expr: Expression =
    mood === "listening" ? "listening" : mood === "speaking" ? "speaking" : mood === "thinking" ? "thinking" : cheer ? "excited" : spawn > 0 ? "excited" : working > 0 ? "working" : sleepy && !hover ? "sleepy" : "happy";
  const transcript = voice.phase === "thinking" && activity.running ? voice.transcript : "";
  const justDone = listed.find((a) => a.status === "done")?.name;
  // The hero card's two lines: what it's doing, and the detail under it.
  // While it listens or answers, bars for the voice take the second line.
  const title =
    mood === "listening" ? (partial || "Listening") : mood === "thinking" ? (transcript ? `“${transcript}”` : slow ? "Still thinking" : "Thinking") : mood === "speaking" ? "Answering" : spawn > 0 ? "Starting agents" : cheer ? "All done!" : waiting > 0 ? `${waiting} ${waiting === 1 ? "agent needs" : "agents need"} you` : working > 0 ? `${working} ${working === 1 ? "agent" : "agents"} working` : sleepy && !hover ? "Zzz…" : "Hi there";
  const line =
    mood === "thinking" ? (slow ? "Still thinking…" : "Thinking…") : spawn > 0 ? "Off they go" : cheer ? (justDone ? `${justDone} finished` : "Nice work") : (agentLine ?? "Ask me anything");
  // Under a notch the idle island hides behind it entirely, so it takes no
  // screen; the face peeks out while agents work, and hovering the notch
  // opens it. Without a notch there's nothing to hide behind.
  const hidden = b.notchEdge === "top" && !!notch && !open && working === 0;
  const shown = listed.length > 3 ? 2 : 3;
  const h = settings.hotkeys;

  return (
    <IslandPanel
      rootRef={root}
      buddy={b}
      notch={notch}
      open={open}
      hidden={hidden}
      busy={busy}
      sleepy={sleepy}
      expr={expr}
      mood={mood}
      cheer={cheer}
      hover={hover}
      title={title}
      line={line}
      level={level}
      working={working}
      queued={activity.queued}
      rows={listed.slice(0, shown).map(rowFor)}
      more={Math.max(0, listed.length - shown)}
      tips={{
        voice: keys(h.voiceAsk),
        text: keys(h.textAsk),
        circle: keys(h.circleToExplain),
        usage: usage && usage.calls > 0 ? `Today · ${usage.calls} ${usage.calls === 1 ? "call" : "calls"} · ${money(usage.cost)}` : "Nothing spent today",
      }}
      spawn={spawn}
      dockSide={settings.agents.dockSide ?? "right"}
      onSpawnEnd={() => setSpawn(0)}
      onHover={() => setHover(true)}
      onAsk={() => api.islandOpen()}
      onAgents={(id) => api.openAgentPanel(id)}
      onSettings={() => api.openSettingsSection("buddy")}
    />
  );
}
