import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { AgentView } from "../bindings/AgentView";
import type { Batch } from "../bindings/Batch";
import type { LiveLine } from "../bindings/LiveLine";
import { api, EVENTS } from "../lib/ipc";
import { useSettings, useTheme } from "../lib/useSettings";
import { inDock, STATUS_LABEL, tone } from "./dockState";

const MAX_CHIPS = 8;
/** A chip the buddy hands off drops in this long after it sets off, ms. */
const HANDOFF_LAND = 650;
/** The drop's length (the `drop` keyframes in dock.css), ms. */
const DROP_MS = 720;

/** Keeps every agent up to date from events. Shared by the dock and panel. */
export function useAgents() {
  const [agents, setAgents] = useState<Map<string, AgentView>>(new Map());
  const [batches, setBatches] = useState<Map<string, Batch>>(new Map());
  const [live, setLive] = useState<Map<string, string>>(new Map());
  useEffect(() => {
    api.agents().then((l) => {
      setAgents(new Map(l.agents.map((a) => [a.id, a])));
      setBatches(new Map(l.batches.map((b) => [b.id, b])));
    });
    const offs = [
      listen<AgentView>(EVENTS.agentUpdate, ({ payload }) => {
        setAgents((m) => new Map(m).set(payload.id, payload));
        // A saved update replaces any passing line.
        setLive((m) => {
          if (!m.has(payload.id)) return m;
          const n = new Map(m);
          n.delete(payload.id);
          return n;
        });
      }),
      listen<string>(EVENTS.agentRemoved, ({ payload }) =>
        setAgents((m) => {
          const n = new Map(m);
          n.delete(payload);
          return n;
        }),
      ),
      listen<Batch>(EVENTS.agentBatch, ({ payload }) => setBatches((m) => new Map(m).set(payload.id, payload))),
      listen<LiveLine>(EVENTS.agentLive, ({ payload }) => setLive((m) => new Map(m).set(payload.id, payload.line))),
    ];
    return () => offs.forEach((o) => void o.then((f) => f()));
  }, []);
  return { agents, batches, live };
}

function useNow(everyMs: number) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), everyMs);
    return () => clearInterval(t);
  }, [everyMs]);
  return now;
}

/**
 * The agent dock (R2): one glowing chip per agent at the screen edge. Hover
 * a chip and its card slides out. Only the chips and the open card take the
 * mouse; the window is exactly their size.
 */
export function Dock() {
  const [settings] = useSettings();
  useTheme(settings);
  const { agents } = useAgents();
  const now = useNow(1000);
  // The card lives in its own window; this only marks its chip.
  const [open, setOpen] = useState<string | null>(null);
  const root = useRef<HTMLDivElement>(null);
  // When each chip first showed up, and how long its drop waits.
  const arrivals = useRef<Map<string, { at: number; delay: number }>>(new Map());
  // When the buddy last set off with a new agent.
  const incomingAt = useRef(-Infinity);

  const side = settings?.agents.dockSide ?? "right";
  const shown = inDock([...agents.values()], now, settings?.agents.doneSeconds ?? 60);
  const chips = shown.slice(-MAX_CHIPS);
  const hidden = shown.length - chips.length;

  useEffect(() => {
    api.dockCardCurrent().then(setOpen, () => {});
    const off = listen<string | null>(EVENTS.dockCard, (e) => setOpen(e.payload));
    const offIncoming = listen(EVENTS.dockIncoming, () => (incomingAt.current = performance.now()));
    return () => {
      off.then((f) => f());
      offIncoming.then((f) => f());
    };
  }, []);

  // The window follows the content's size; Rust keeps it on the edge. The
  // root element changes when the dock empties or fills, so re-attach then.
  const empty = shown.length === 0;
  useEffect(() => {
    const el = root.current;
    if (empty || !el) {
      api.dockLayout(0, 0).catch(() => {});
      return;
    }
    const report = () => {
      const r = el.getBoundingClientRect();
      api.dockLayout(Math.ceil(r.width), Math.ceil(r.height)).catch(() => {});
    };
    const ro = new ResizeObserver(report);
    ro.observe(el);
    report();
    return () => ro.disconnect();
  }, [empty]);

  const hover = (id: string, chip: HTMLElement) => {
    const r = chip.getBoundingClientRect();
    api.dockCardShow(id, r.top + r.height / 2);
    if (agents.get(id)?.unseen) api.agentSeen(id);
  };

  // New agents arrive with a little flourish (the end of the hand-off),
  // held back until the buddy's copy lands when it carried them here. The
  // drop's delay while it plays, else null.
  const arrived = (id: string) => {
    const now = performance.now();
    let a = arrivals.current.get(id);
    if (!a) {
      a = { at: now, delay: Math.max(0, HANDOFF_LAND - (now - incomingAt.current)) };
      arrivals.current.set(id, a);
    }
    return now < a.at + a.delay + DROP_MS ? a.delay : null;
  };

  if (empty) return <div ref={root} className="dock dock--empty" />;

  return (
    <div
      ref={root}
      className={`dock dock--${side}`}
      onMouseEnter={() => api.dockCardHover("dock", true)}
      onMouseLeave={() => api.dockCardHover("dock", false)}
    >
      <div className="chips">
        {hidden > 0 && (
          <button type="button" className="chip chip--more" onClick={() => api.openAgentPanel()} title="Open the agent panel">
            +{hidden}
          </button>
        )}
        {chips.map((a) => {
          const drop = arrived(a.id);
          return (
            <button
              key={a.id}
              type="button"
              className={`chip chip--${tone(a.status)}${drop !== null ? " chip--new" : ""}${open === a.id ? " is-open" : ""}`}
              // The drop, then the glow that follows it.
              style={drop ? { animationDelay: `${drop}ms, ${drop + DROP_MS}ms` } : undefined}
              aria-label={`${a.name}: ${STATUS_LABEL[a.status]}`}
              onMouseEnter={(e) => hover(a.id, e.currentTarget)}
              onFocus={(e) => hover(a.id, e.currentTarget)}
              // Toggles the card; the card's ↗ button opens the panel.
              onClick={(e) => (open === a.id ? api.dockCardClose() : hover(a.id, e.currentTarget))}
            >
              <Mark />
              {/* Remounts on every status change, so each change ripples once. */}
              <span key={a.status} className="chip__ripple" aria-hidden="true" />
              {a.unseen && <span className="chip__dot" />}
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** Helpy's pointer shape, filled with the status colour. */
function Mark() {
  return (
    <svg viewBox="0 0 64 64" width="24" height="24" aria-hidden="true">
      <path d="M8 8 L27 15 A21 21 0 1 1 15 27 Z" />
    </svg>
  );
}
