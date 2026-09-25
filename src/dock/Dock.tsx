import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import { listen } from "@tauri-apps/api/event";
import type { AgentView } from "../bindings/AgentView";
import type { Batch } from "../bindings/Batch";
import type { LiveLine } from "../bindings/LiveLine";
import { api, EVENTS } from "../lib/ipc";
import { useSettings, useTheme } from "../lib/useSettings";
import { duration, inDock, lastCommand, spend, STATUS_LABEL, tone } from "./dockState";

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
  useTheme(settings);
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

  return (
    <div ref={root} className={`dock dock--${side}`} onMouseLeave={closeSoon} onMouseEnter={keepOpen}>
      {current && (
        <Card key={current.id} agent={current} line={live.get(current.id)} onPin={pin} onClose={() => setOpen(null)} />
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
            {/* Remounts on every status change, so each change ripples once. */}
            <span key={a.status} className="chip__ripple" aria-hidden="true" />
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

const FINISHED = ["done", "ready", "failed", "stopped", "cancelled"];

function Card(props: { agent: AgentView; line: string | undefined; onPin: (on: boolean) => void; onClose: () => void }) {
  const { agent: a, line } = props;
  const t = tone(a.status);
  const cmd = lastCommand(a);
  const working = a.status === "running" || a.status === "queued" || a.status === "paused";
  const progress = Math.min(1, a.counters.steps / Math.max(1, a.maxSteps));
  const done = (a.status === "done" || a.status === "ready") && !!a.result;

  return (
    <div className="stack">
      <section className={`card card--${t}`} aria-label={a.name}>
        <header className="card__head">
          <span className="avatar" aria-hidden="true">
            <i />
          </span>
          <div className="card__title">
            <h2 title={a.goal}>{a.name}</h2>
            <span className="card__state">
              {STATUS_LABEL[a.status]}
              {FINISHED.includes(a.status) && ` · ${duration(a.activeMs)}`}
            </span>
          </div>
          {(a.status === "running" || a.status === "queued") && (
            <IconButton label="Pause" onClick={() => api.agentPause(a.id)} path="M7 5v10M13 5v10" />
          )}
          {a.status === "paused" && <IconButton label="Resume" onClick={() => api.agentResume(a.id)} path="M7 5l8 5-8 5z" />}
          {!FINISHED.includes(a.status) && <IconButton label="Cancel the agent" onClick={() => api.agentCancel(a.id)} path="M6 6h8v8H6z" />}
          <IconButton label="Open in the agent panel" onClick={() => api.openAgentPanel(a.id)} path="M7 13l6-6M8 7h5v5" />
          <IconButton
            label="Remove from the dock (the agent keeps going)"
            onClick={() => {
              props.onClose();
              api.agentDismiss(a.id);
            }}
            path="m6.5 6.5 7 7M13.5 6.5l-7 7"
          />
        </header>

        {working && (
          <>
            <p className="card__status">
              <span key={line ?? a.statusLine} className="rise">
                {line ?? (a.statusLine || "Getting started…")}
              </span>
              {a.status !== "paused" && <Typing />}
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
            {a.pending.detail && <pre className="card__detail">{a.pending.detail}</pre>}
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
              <div className="nexts">
                {a.pending.options.map((o, i) => (
                  <button key={o} type="button" className="next" style={{ ["--i" as string]: i }} onClick={() => api.agentAnswer(a.id, { type: "choice", text: o })}>
                    <span>{o}</span>
                    <Arrow />
                  </button>
                ))}
              </div>
            )}
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

        {done && (
          <>
            <div className="card__result rise">
              <Markdown components={{ a: ({ children }) => <span>{children}</span>, img: () => null }}>{a.result}</Markdown>
            </div>
            {a.suggestions.length > 0 && (
              <div className="nexts">
                {a.suggestions.map((s, i) => (
                  <button key={s} type="button" className="next" style={{ ["--i" as string]: i }} onClick={() => api.agentFollowUp(a.id, s)}>
                    <span>{s}</span>
                    <Arrow />
                  </button>
                ))}
              </div>
            )}
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

        <footer className="card__foot">
          {a.changes > 0 && FINISHED.includes(a.status) ? (
            <button type="button" className="card__undo" onClick={() => api.agentUndo(a.id)}>
              Undo {a.changes} file {a.changes === 1 ? "change" : "changes"}
            </button>
          ) : (
            <span />
          )}
          <span className="card__meta">{spend(a)}</span>
        </footer>
      </section>

      {done && (
        <FollowBar
          agent={a}
          placeholder={a.keepOpen ? "What should change?" : "Ask a follow-up…"}
          onSend={(text) => api.agentFollowUp(a.id, text)}
          onVoice={() => api.agentVoiceFollowUp(a.id)}
          onPin={props.onPin}
        />
      )}
      {working && (
        <FollowBar
          agent={a}
          placeholder="Tell it something…"
          onSend={(text) => api.agentSteer(a.id, text)}
          onVoice={() => api.agentVoiceFollowUp(a.id)}
          onPin={props.onPin}
        />
      )}
      {a.pending?.type === "question" && (
        <FollowBar agent={a} placeholder="Or type an answer…" onSend={(text) => api.agentAnswer(a.id, { type: "choice", text })} onPin={props.onPin} />
      )}
    </div>
  );
}

/** The split view's second piece: a follow-up bar floating under the card. */
function FollowBar(props: { agent: AgentView; placeholder: string; onSend: (text: string) => void; onVoice?: () => void; onPin: (on: boolean) => void }) {
  const { agent: a, onPin } = props;
  const [text, setText] = useState("");
  const [sent, setSent] = useState(0);
  const send = () => {
    if (!text.trim()) return;
    props.onSend(text.trim());
    setText("");
    setSent((n) => n + 1);
    onPin(false);
  };
  return (
    <form
      className={`followbar card--${tone(a.status)}`}
      onSubmit={(e) => {
        e.preventDefault();
        send();
      }}
    >
      <input
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder={props.placeholder}
        onFocus={() => onPin(true)}
        onBlur={() => onPin(false)}
        onKeyDown={(e) => e.key === "Escape" && (e.currentTarget.blur(), onPin(false))}
      />
      {props.onVoice && (
        <button type="button" className="round" aria-label="Follow up by voice" title="Follow up by voice" onClick={props.onVoice}>
          <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true">
            <rect x="7.5" y="3" width="5" height="9" rx="2.5" />
            <path d="M5 10a5 5 0 0 0 10 0M10 15v2.5" />
          </svg>
        </button>
      )}
      <button key={sent} type="submit" className="round round--send" aria-label="Send" disabled={!text.trim()}>
        <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true">
          <path d="M10 15.5v-11M5.5 9 10 4.5 14.5 9" />
        </svg>
      </button>
    </form>
  );
}

function Typing() {
  return (
    <span className="typing" aria-hidden="true">
      <i />
      <i />
      <i />
    </span>
  );
}

function Arrow() {
  return (
    <svg className="next__arrow" viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
      <path d="M4.5 10h11M11 5.5l4.5 4.5-4.5 4.5" />
    </svg>
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
