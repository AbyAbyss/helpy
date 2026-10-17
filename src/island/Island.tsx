import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AskActivity } from "../bindings/AskActivity";
import type { VoicePhase } from "../bindings/VoicePhase";
import { Buddy, type BuddyState } from "../buddy/Buddy";
import { api, EVENTS } from "../lib/ipc";
import { useSettings } from "../lib/useSettings";

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
  const [working, setWorking] = useState(0);
  // A little agent pushed off toward the dock as agents start.
  const [spawn, setSpawn] = useState(0);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const offs = [
      listen<VoicePhase>(EVENTS.voiceState, (e) => setVoice(e.payload.phase)),
      listen<boolean>(EVENTS.speaking, (e) => setSpeaking(e.payload)),
      listen<AskActivity>(EVENTS.askActivity, (e) => setActivity(e.payload)),
      listen<number>(EVENTS.agentCount, (e) => setWorking(e.payload)),
      getCurrentWindow().listen(EVENTS.islandSpawn, () => setSpawn((n) => n + 1)),
    ];
    api.askActivity().then(setActivity, () => {});
    api.agents().then((l) => setWorking(l.agents.filter((a) => !["done", "failed", "stopped", "cancelled", "ready"].includes(a.status)).length), () => {});
    return () => offs.forEach((o) => void o.then((f) => f()));
  }, []);

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
  }, [b?.home, b?.enabled, b?.notchEdge, b?.notchOffset]);

  if (!b) return null;
  const mood: BuddyState =
    voice === "listening" ? "listening" : speaking ? "speaking" : voice === "transcribing" || activity.running ? "thinking" : "idle";
  const busy = mood !== "idle";
  const open = hover || busy || spawn > 0;
  const dockSide = settings?.agents.dockSide ?? "right";
  const line =
    spawn > 0 ? "Starting agents…" : mood === "listening" ? "Listening…" : mood === "thinking" ? "Thinking…" : mood === "speaking" ? "Speaking" : working > 0 ? `${working} ${working === 1 ? "agent" : "agents"} working` : "Ask me anything";

  return (
    <div
      ref={root}
      className={`island island--${b.notchEdge}${open ? " is-open" : ""}${busy ? " is-busy" : ""}`}
      role="button"
      aria-label="Helpy. Click to ask"
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      onClick={() => api.islandOpen()}
    >
      {open ? (
        <>
          <span className={`island__face${spawn > 0 ? " is-pushing" : ""}`} key={spawn}>
            <Buddy style={b.style === "custom" ? "pip" : b.style} size={28} state={mood} animate={b.showStateAnimations} badge={b.showAgentBadge && working > 0 ? working : null} queued={activity.queued} />
            {spawn > 0 && (
              <span className={`island__spawn island__spawn--${dockSide}`} onAnimationEnd={() => setSpawn(0)}>
                <Buddy style={b.style === "custom" ? "pip" : b.style} size={16} state="idle" animate={false} />
              </span>
            )}
          </span>
          <span className="island__line">{line}</span>
        </>
      ) : (
        <span className="island__eyes" aria-hidden="true">
          <i />
          <i />
          {working > 0 && <b className="island__dot" />}
        </span>
      )}
    </div>
  );
}
