import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AgentView } from "../bindings/AgentView";
import type { AskActivity } from "../bindings/AskActivity";
import type { VoicePhase } from "../bindings/VoicePhase";
import { Buddy, type BuddyState } from "../buddy/Buddy";
import { api, EVENTS } from "../lib/ipc";
import { useSettings } from "../lib/useSettings";

export type Expression = "happy" | "listening" | "thinking" | "speaking" | "working" | "excited" | "sleepy";

/**
 * The island as a character: white eyes with dark pupils, a small mouth and
 * two hands on the rim, drawn in a 60 by 30 box. Each expression changes
 * the eyes and mouth; the extras (a question mark, sparks, sound waves,
 * a progress bar, z's) sit around them.
 */
function PillFace({ expr }: { expr: Expression }) {
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
      <circle cx={9} cy={27.5} r={4} fill="#1c2030" />
      <circle cx={51} cy={27.5} r={4} fill="#1c2030" />
      <circle cx={9} cy={26.5} r={3} fill="#262b3b" />
      <circle cx={51} cy={26.5} r={3} fill="#262b3b" />
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

/**
 * The buddy's home at the screen edge: a small dark island that hugs the
 * edge (under a MacBook's notch by default). Idle it shows only its eyes;
 * hovered, or while Helpy listens, thinks or speaks, it opens into the face
 * and a line of status. A click opens the text chat. Rust keeps the window
 * exactly this size and against the chosen edge.
 */
export function Island() {
  const [settings] = useSettings();
  const [hover, setHover] = useState(false);
  const [voice, setVoice] = useState<VoicePhase["phase"]>("idle");
  const [speaking, setSpeaking] = useState(false);
  const [activity, setActivity] = useState<AskActivity>({ running: false, queued: 0 });
  // Agents by id, for how many work and what each is doing right now.
  const [agents, setAgents] = useState<Map<string, AgentView>>(new Map());
  const known = useRef<Map<string, AgentView>>(new Map());
  // A step an agent just reported, shown for a moment; and which running
  // agent the open island narrates.
  const [toast, setToast] = useState<string | null>(null);
  const [narrated, setNarrated] = useState(0);
  const lastToast = useRef(-Infinity);
  // A model that's taking longer than usual to answer.
  const [slow, setSlow] = useState(false);
  // A little agent pushed off toward the dock as agents start.
  const [spawn, setSpawn] = useState(0);
  // The last agent just finished: a moment of cheer.
  const [cheer, setCheer] = useState(false);
  // The mouse has rested a while: it dozes off.
  const [sleepy, setSleepy] = useState(false);
  const wasWorking = useRef(0);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const offs = [
      listen<VoicePhase>(EVENTS.voiceState, (e) => setVoice(e.payload.phase)),
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
      listen<boolean>(EVENTS.sleepy, (e) => setSleepy(e.payload)),
    ];
    api.askActivity().then(setActivity, () => {});
    api.agents().then((l) => {
      known.current = new Map(l.agents.map((a) => [a.id, a]));
      setAgents(known.current);
    }, () => {});
    return () => offs.forEach((o) => void o.then((f) => f()));
  }, []);
  const running = [...agents.values()].filter((a) => a.status === "running" && !a.parent);
  const working = [...agents.values()].filter((a) => !["done", "failed", "stopped", "cancelled", "ready"].includes(a.status) && !a.parent).length;

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
    if (!el) return;
    const report = () => {
      const r = el.getBoundingClientRect();
      api.islandLayout(Math.ceil(r.width), Math.ceil(r.height)).catch(() => {});
    };
    const ro = new ResizeObserver(report);
    ro.observe(el);
    report();
    return () => ro.disconnect();
  }, [b?.home, b?.enabled, b?.notchEdge, b?.notchOffset, b?.islandSize]);

  const mood: BuddyState =
    voice === "listening" ? "listening" : speaking ? "speaking" : voice === "transcribing" || activity.running ? "thinking" : "idle";
  useEffect(() => {
    setSlow(false);
    if (mood !== "thinking") return;
    const t = setTimeout(() => setSlow(true), 5000);
    return () => clearTimeout(t);
  }, [mood]);

  if (!b) return null;
  const busy = mood !== "idle";
  const open = hover || busy || spawn > 0 || cheer || !!toast;
  const dockSide = settings?.agents.dockSide ?? "right";
  // What the open island says about its agents: a step just reported, or
  // the working agents one at a time.
  const now = running[narrated % Math.max(running.length, 1)];
  const agentLine = toast ?? (now?.statusLine ? `${now.name} · ${now.statusLine}` : working > 0 ? `${working} ${working === 1 ? "agent" : "agents"} working` : null);
  const expr: Expression =
    mood === "listening" ? "listening" : mood === "speaking" ? "speaking" : mood === "thinking" ? "thinking" : cheer ? "excited" : spawn > 0 ? "excited" : working > 0 ? "working" : sleepy && !hover ? "sleepy" : "happy";
  const line =
    spawn > 0 ? "Starting agents…" : cheer ? "All done!" : mood === "listening" ? "Listening…" : mood === "thinking" ? (slow ? "Still thinking…" : "Thinking…") : mood === "speaking" ? "Speaking" : (agentLine ?? "Ask me anything");

  return (
    <div
      ref={root}
      className={`island island--${b.notchEdge} island--${b.islandLook}${open ? " is-open" : ""}${busy ? " is-busy" : ""}${sleepy && !open ? " is-sleepy" : ""}`}
      style={{ zoom: b.islandSize }}
      role="button"
      aria-label="Helpy. Click to ask"
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      onClick={() => api.islandOpen()}
    >
      {open ? (
        <>
          <span className={`island__face${spawn > 0 ? " is-pushing" : ""}`} key={spawn}>
            {b.islandLook === "face" ? (
              <PillFace expr={expr} />
            ) : (
              <Buddy style={b.style === "custom" ? "pip" : b.style} size={28} state={mood} mood={cheer ? "happy" : sleepy && !hover ? "sleepy" : null} animate={b.showStateAnimations} badge={b.showAgentBadge && working > 0 ? working : null} queued={activity.queued} />
            )}
            {spawn > 0 && (
              <span className={`island__spawn island__spawn--${dockSide}`} onAnimationEnd={() => setSpawn(0)}>
                <Buddy style={b.style === "custom" ? "pip" : b.style} size={16} baby state="idle" animate={false} />
              </span>
            )}
          </span>
          <span className="island__line" title={line}>{line}</span>
        </>
      ) : b.islandLook === "peek" ? (
        <span className="island__peek" aria-hidden="true">
          <Buddy style={b.style === "custom" ? "pip" : b.style} size={22} baby state="idle" mood={sleepy ? "sleepy" : null} animate={b.showStateAnimations} />
          {working > 0 && <b className="island__dot" />}
        </span>
      ) : (
        <span className="island__rest" aria-hidden="true">
          <PillFace expr={expr} />
        </span>
      )}
    </div>
  );
}
