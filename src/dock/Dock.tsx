import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import { listen } from "@tauri-apps/api/event";
import type { AgentView } from "../bindings/AgentView";
import type { Batch } from "../bindings/Batch";
import type { LiveLine } from "../bindings/LiveLine";
import { api, EVENTS } from "../lib/ipc";
import { useSettings } from "../lib/useSettings";
import { inDock, lastCommand, spend, STATUS_LABEL, tone } from "./dockState";

const MAX_CHIPS = 8;

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
  const { agents, live } = useAgents();
  const now = useNow(1000);
  const [open, setOpen] = useState<string | null>(null);
  const [pinned, setPinned] = useState(false);
  const closeTimer = useRef<number>(0);
  const root = useRef<HTMLDivElement>(null);
  const seenIds = useRef<Set<string>>(new Set());

  const side = settings?.agents.dockSide ?? "right";
  const shown = inDock([...agents.values()], now, settings?.agents.doneSeconds ?? 60);
  const chips = shown.slice(-MAX_CHIPS);
  const hidden = shown.length - chips.length;
  const current = open ? agents.get(open) : undefined;

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

  const keepOpen = () => window.clearTimeout(closeTimer.current);
  const closeSoon = () => {
    if (pinned) return;
    window.clearTimeout(closeTimer.current);
    closeTimer.current = window.setTimeout(() => setOpen(null), 260);
  };
  const hover = (id: string) => {
    keepOpen();
    setOpen(id);
    if (agents.get(id)?.unseen) api.agentSeen(id);
  };
  const pin = (on: boolean) => {
    setPinned(on);
    api.dockFocus(on);
    if (!on) closeSoon();
  };

  // New agents arrive with a little flourish (the end of the hand-off).
  const arrived = (id: string) => {
    if (seenIds.current.has(id)) return false;
    seenIds.current.add(id);
    return true;
  };

  if (empty) return <div ref={root} className="dock dock--empty" />;

  const chipIndex = open ? chips.findIndex((a) => a.id === open) : -1;

  return (
    <div ref={root} className={`dock dock--${side}`} onMouseLeave={closeSoon} onMouseEnter={keepOpen}>
      {current && (
        <Card
          key={current.id}
          agent={current}
          line={live.get(current.id)}
          arrowAt={chipIndex}
          count={chips.length + (hidden ? 1 : 0)}
          onPin={pin}
          onClose={() => setOpen(null)}
        />
      )}
      <div className="chips">
        {hidden > 0 && (
          <button type="button" className="chip chip--more" onClick={() => api.openAgentPanel()} title="Open the agent panel">
            +{hidden}
          </button>
        )}
        {chips.map((a) => (
          <button
            key={a.id}
            type="button"
            className={`chip chip--${tone(a.status)}${arrived(a.id) ? " chip--new" : ""}${open === a.id ? " is-open" : ""}`}
            aria-label={`${a.name}: ${STATUS_LABEL[a.status]}`}
            onMouseEnter={() => hover(a.id)}
            onFocus={() => hover(a.id)}
            onClick={() => api.openAgentPanel(a.id)}
          >
            <Mark />
            {a.unseen && <span className="chip__dot" />}
          </button>
        ))}
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

function Card(props: {
  agent: AgentView;
  line: string | undefined;
  arrowAt: number;
  count: number;
  onPin: (on: boolean) => void;
  onClose: () => void;
}) {
  const { agent: a, line, arrowAt, count } = props;
  const t = tone(a.status);
  const cmd = lastCommand(a);
  const working = a.status === "running" || a.status === "queued" || a.status === "paused";
  const progress = Math.min(1, a.counters.steps / Math.max(1, a.maxSteps));
  // Point at the hovered chip: chips are 52 px apart, centred in the column.
  const arrowY = arrowAt >= 0 ? `calc(50% + ${(arrowAt - (count - 1) / 2) * 52}px)` : "50%";

  return (
    <section className={`card card--${t}`} style={{ ["--arrow-y" as string]: arrowY }} aria-label={a.name}>
      <header className="card__head">
        <h2 title={a.goal}>{a.name}</h2>
        <span className="pill">{STATUS_LABEL[a.status]}</span>
        <span className="card__spacer" />
        {(a.status === "running" || a.status === "queued") && (
          <IconButton label="Pause" onClick={() => api.agentPause(a.id)} path="M7 5v10M13 5v10" />
        )}
        {a.status === "paused" && <IconButton label="Resume" onClick={() => api.agentResume(a.id)} path="M7 5l8 5-8 5z" />}
        {!["done", "failed", "stopped", "cancelled", "ready"].includes(a.status) && (
          <IconButton label="Cancel the agent" onClick={() => api.agentCancel(a.id)} path="M6 6h8v8H6z" />
        )}
        <IconButton label="Open in the agent panel" onClick={() => api.openAgentPanel(a.id)} path="M8 5h7v7M15 5l-9 9" />
        <IconButton
          label="Remove from the dock (the agent keeps going)"
          onClick={() => {
            props.onClose();
            api.agentDismiss(a.id);
          }}
          path="m6 6 8 8M14 6l-8 8"
        />
      </header>

      {working && (
        <>
          <p className="card__status">
            <Bubble />
            <span key={line ?? a.statusLine} className="fade">
              {line ?? (a.statusLine || "Getting started…")}
            </span>
          </p>
          {cmd && (
            <p className="card__cmd">
              <svg viewBox="0 0 20 20" width="13" height="13" aria-hidden="true">
                <path d="m4 6 4 4-4 4M10 14h6" />
              </svg>
              <code>{cmd}</code>
            </p>
          )}
          <div className="bar" aria-label={`Step ${a.counters.steps} of ${a.maxSteps}`}>
            <span style={{ width: `${Math.max(4, progress * 100)}%` }} />
          </div>
        </>
      )}

      {a.pending?.type === "approval" && (
        <div className="card__ask">
          <p className="card__summary">{a.pending.summary}</p>
          {a.pending.detail &&
            (a.pending.kind === "shell" || a.pending.kind === "fileChange" ? (
              <pre className="card__detail">{a.pending.detail}</pre>
            ) : (
              <p className="card__note">{a.pending.detail}</p>
            ))}
          <div className="card__buttons">
            <button type="button" className="btn btn--primary" onClick={() => api.agentAnswer(a.id, { type: "approve" })}>
              Approve
            </button>
            <button type="button" className="btn" onClick={() => api.agentAnswer(a.id, { type: "reject", note: null })}>
              Reject
            </button>
            <button type="button" className="btn btn--quiet" onClick={() => api.openAgentPanel("inbox")}>
              Review
            </button>
          </div>
        </div>
      )}

      {a.pending?.type === "question" && (
        <div className="card__ask">
          <p className="card__summary">{a.pending.question}</p>
          {a.pending.options.length > 0 && (
            <div className="card__buttons card__buttons--wrap">
              {a.pending.options.map((o) => (
                <button key={o} type="button" className="btn" onClick={() => api.agentAnswer(a.id, { type: "choice", text: o })}>
                  {o}
                </button>
              ))}
            </div>
          )}
          <TextReply placeholder="Or type an answer…" onPin={props.onPin} onSend={(text) => api.agentAnswer(a.id, { type: "choice", text })} />
        </div>
      )}

      {a.pending?.type === "failure" && (
        <div className="card__ask">
          <p className="card__summary">{a.pending.message}</p>
          <div className="card__buttons">
            <button type="button" className="btn btn--primary" onClick={() => api.agentAnswer(a.id, { type: "retry" })}>
              Retry
            </button>
            <button type="button" className="btn" onClick={() => api.agentAnswer(a.id, { type: "skip" })}>
              Skip this step
            </button>
            <button type="button" className="btn btn--quiet" onClick={() => api.agentAnswer(a.id, { type: "cancel" })}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {(a.status === "done" || a.status === "ready") && a.result && (
        <>
          <div className="card__result">
            <Markdown components={{ a: ({ children }) => <span>{children}</span>, img: () => null }}>{a.result}</Markdown>
          </div>
          {a.suggestions.length > 0 && (
            <div className="card__next">
              <span className="card__label">Suggested next</span>
              <div className="card__buttons card__buttons--wrap">
                {a.suggestions.map((s) => (
                  <button key={s} type="button" className="chipb" onClick={() => api.agentFollowUp(a.id, s)}>
                    {s}
                  </button>
                ))}
              </div>
            </div>
          )}
          <FollowUp agent={a} onPin={props.onPin} />
        </>
      )}

      {(a.status === "failed" || a.status === "stopped") && (
        <div className="card__ask">
          <p className="card__summary card__summary--err">{a.stop?.message ?? a.error}</p>
          <div className="card__buttons">
            {a.stop && (a.stop.limit === "agentBudget" || a.stop.limit === "batchBudget") && a.counters.extraTokens === 0 && (
              <button type="button" className="btn btn--primary" onClick={() => api.agentRaise(a.id)}>
                Raise the limit and continue once
              </button>
            )}
            <button type="button" className="btn" onClick={() => api.agentRetry(a.id)}>
              Retry
            </button>
          </div>
        </div>
      )}

      {a.changes > 0 && ["done", "ready", "failed", "stopped", "cancelled"].includes(a.status) && (
        <button type="button" className="card__undo" onClick={() => api.agentUndo(a.id)}>
          Undo {a.changes} file {a.changes === 1 ? "change" : "changes"}
        </button>
      )}

      <footer className="card__meta">{spend(a)}</footer>
    </section>
  );
}

function FollowUp({ agent: a, onPin }: { agent: AgentView; onPin: (on: boolean) => void }) {
  const [typing, setTyping] = useState(false);
  if (typing)
    return (
      <TextReply
        placeholder={a.keepOpen ? "What should change?" : "Ask for more…"}
        autoFocus
        onPin={(on) => {
          onPin(on);
          if (!on) setTyping(false);
        }}
        onSend={(text) => api.agentFollowUp(a.id, text)}
      />
    );
  return (
    <div className="card__follow">
      <span className="card__label">{a.keepOpen ? "Need changes?" : "Follow up"}</span>
      <button type="button" className="btn" onClick={() => setTyping(true)}>
        Text
      </button>
      <button type="button" className="btn" onClick={() => api.agentVoiceFollowUp(a.id)}>
        Voice
      </button>
    </div>
  );
}

function TextReply(props: { placeholder: string; autoFocus?: boolean; onPin: (on: boolean) => void; onSend: (text: string) => void }) {
  const [text, setText] = useState("");
  return (
    <form
      className="reply"
      onSubmit={(e) => {
        e.preventDefault();
        if (!text.trim()) return;
        props.onSend(text.trim());
        setText("");
        props.onPin(false);
      }}
    >
      <input
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder={props.placeholder}
        autoFocus={props.autoFocus}
        onFocus={() => props.onPin(true)}
        onBlur={() => props.onPin(false)}
        onKeyDown={(e) => e.key === "Escape" && (e.currentTarget.blur(), props.onPin(false))}
      />
      <button type="submit" className="btn btn--primary" disabled={!text.trim()}>
        Send
      </button>
    </form>
  );
}

function IconButton({ label, path, onClick }: { label: string; path: string; onClick: () => void }) {
  return (
    <button type="button" className="icon" aria-label={label} title={label} onClick={onClick}>
      <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
        <path d={path} />
      </svg>
    </button>
  );
}

function Bubble() {
  return (
    <svg className="card__bubble" viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
      <path d="M4 4.5h12a1.5 1.5 0 0 1 1.5 1.5v6.5A1.5 1.5 0 0 1 16 14H9l-3.5 3v-3H4a1.5 1.5 0 0 1-1.5-1.5V6A1.5 1.5 0 0 1 4 4.5z" />
    </svg>
  );
}
