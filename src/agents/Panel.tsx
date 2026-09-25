import { useEffect, useMemo, useState } from "react";
import Markdown from "react-markdown";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import type { AgentView } from "../bindings/AgentView";
import type { Batch } from "../bindings/Batch";
import type { LogKind } from "../bindings/LogKind";
import { useAgents } from "../dock/Dock";
import { AgentFiles } from "../dock/Files";
import { duration, STATUS_LABEL, tokens, tone } from "../dock/dockState";
import { api, EVENTS } from "../lib/ipc";
import { ApprovalBox, Inbox } from "./Approvals";
import { Templates } from "./Templates";
import { useSettings, useTheme } from "../lib/useSettings";

type Filter = "active" | "finished" | "all";
const FINISHED = ["done", "failed", "stopped", "cancelled"];
const MODE_LABEL = { single: "One agent", parallel: "All at once", sequential: "One after another" } as const;

/** The agent panel: every agent, grouped by the request that started it. */
export function Panel() {
  const [settings] = useSettings();
  useTheme(settings);
  const { agents, batches, live } = useAgents();
  const [filter, setFilter] = useState<Filter>("all");
  const [selected, setSelected] = useState<string | null>(null);
  // What the right side shows instead of an agent.
  const [view, setView] = useState<"agent" | "inbox" | "templates">("agent");
  const inbox = view === "inbox";
  const setInbox = (on: boolean) => setView(on ? "inbox" : "agent");

  useEffect(() => {
    const off = listen<string>(EVENTS.agentsFocus, (e) => {
      // "inbox" opens the approval inbox instead of an agent.
      const isInbox = e.payload === "inbox";
      setInbox(isInbox);
      setSelected(isInbox ? null : e.payload);
      setFilter("all");
    });
    return () => void off.then((f) => f());
  }, []);

  const groups = useMemo(() => {
    const list = [...agents.values()].filter((a) =>
      filter === "all" ? true : filter === "finished" ? FINISHED.includes(a.status) : !FINISHED.includes(a.status),
    );
    const by = new Map<string, AgentView[]>();
    // Helpers sit right under the agent that started them.
    const rank = (a: AgentView) => (a.parent ? agents.get(a.parent) ?? a : a);
    const sorted = list.sort((x, y) => rank(x).order - rank(y).order || rank(x).created - rank(y).created || Number(!!x.parent) - Number(!!y.parent) || x.created - y.created);
    for (const a of sorted) by.set(a.batch, [...(by.get(a.batch) ?? []), a]);
    return [...by.entries()]
      .map(([id, list]) => ({ batch: batches.get(id), id, list }))
      .sort((a, b) => (b.batch?.created ?? 0) - (a.batch?.created ?? 0));
  }, [agents, batches, filter]);

  // Native glass (macOS vibrancy, Windows Mica) shows through when it's on.
  useEffect(() => {
    const root = document.documentElement;
    if (navigator.platform.includes("Mac")) root.setAttribute("data-mac", "");
    api.windowGlass().then((g) => g && root.setAttribute("data-glass", g), () => {});
  }, []);

  const current = selected ? agents.get(selected) : undefined;
  const waiting = [...agents.values()].filter((a) => a.pending?.type === "approval").length;
  useEffect(() => {
    if (current?.unseen) api.agentSeen(current.id);
  }, [current]);

  return (
    <div className="ap">
      <aside className="ap__list">
        <header className="ap__head" data-tauri-drag-region>
          <h1>Agents</h1>
          <div className="seg" role="radiogroup" aria-label="Show">
            {(["active", "finished", "all"] as const).map((f) => (
              <button key={f} type="button" role="radio" aria-checked={filter === f} onClick={() => setFilter(f)}>
                {f[0].toUpperCase() + f.slice(1)}
              </button>
            ))}
          </div>
        </header>
        <NewTask />
        <button type="button" className={`row inboxrow${view === "templates" ? " is-on" : ""}`} onClick={() => setView("templates")}>
          <span className="dot dot--templates" aria-hidden="true" />
          <span className="row__main">
            <span className="row__name">Templates</span>
          </span>
          <span className="row__status">Start one</span>
        </button>
        <button type="button" className={`row inboxrow${inbox ? " is-on" : ""}`} onClick={() => setInbox(true)}>
          <span className={`dot dot--${waiting ? "alert" : "idle"}`} aria-hidden="true" />
          <span className="row__main">
            <span className="row__name">Approval inbox</span>
          </span>
          <span className="row__status">{waiting ? `${waiting} waiting` : "Empty"}</span>
        </button>
        <div className="ap__groups">
          {groups.length === 0 && <p className="ap__empty">{filter === "active" ? "No agents at work." : "No agents yet."}</p>}
          {groups.map((g) => (
            <section key={g.id} className="group">
              <h2 className="group__title" title={g.batch?.request}>
                <span>{g.batch?.request || "Agents"}</span>
                {g.list.length > 1 && g.batch && <small>{MODE_LABEL[g.batch.mode]}</small>}
              </h2>
              {g.list.map((a) => (
                <button
                  key={a.id}
                  type="button"
                  className={`row${a.parent ? " row--helper" : ""}${view === "agent" && selected === a.id ? " is-on" : ""}`}
                  onClick={() => {
                    setSelected(a.id);
                    setInbox(false);
                  }}
                >
                  <span className={`dot dot--${tone(a.status)}`} aria-hidden="true" />
                  <span className="row__main">
                    <span className="row__name">
                      {a.name}
                      {a.unseen && <span className="row__new">new</span>}
                    </span>
                    <span className="row__line">{live.get(a.id) ?? (a.result && FINISHED.concat("ready").includes(a.status) ? firstLine(a.result) : a.statusLine)}</span>
                  </span>
                  <span className="row__status">{STATUS_LABEL[a.status]}</span>
                </button>
              ))}
            </section>
          ))}
        </div>
      </aside>
      <main className="ap__detail">
        {view === "templates" ? (
          <Templates />
        ) : inbox ? (
          <Inbox agents={[...agents.values()]} />
        ) : current ? (
          <Detail key={current.id} agent={current} batch={batches.get(current.batch)} line={live.get(current.id)} agentName={(id) => agents.get(id)?.name ?? "an earlier agent"} />
        ) : (
          <div className="ap__placeholder">
            <p>Pick an agent to see what it's doing, or describe a new task on the left.</p>
            <p className="muted">You can also ask for one out loud: “research standing desks under £400 and make me a shortlist”.</p>
          </div>
        )}
      </main>
    </div>
  );
}

function NewTask() {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const plan = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.plan(text);
      setText("");
    } catch (e) {
      setError(String(e));
    }
    setBusy(false);
  };
  return (
    <form
      className="newtask"
      onSubmit={(e) => {
        e.preventDefault();
        plan();
      }}
    >
      <textarea
        value={text}
        rows={2}
        placeholder="Describe a task for an agent…"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            plan();
          }
        }}
      />
      <div className="newtask__foot">
        {error ? <span className="err-text">{error}</span> : <span className="muted">Helpy shows a plan before anything starts.</span>}
        <button type="submit" className="btn btn--primary" disabled={!text.trim() || busy}>
          {busy ? "Planning…" : "Plan it"}
        </button>
      </div>
    </form>
  );
}

function Detail({ agent: a, batch, line, agentName }: { agent: AgentView; batch: Batch | undefined; line: string | undefined; agentName: (id: string) => string }) {
  const [answer, setAnswer] = useState("");
  const [followUp, setFollowUp] = useState("");
  const [renaming, setRenaming] = useState(false);
  const [name, setName] = useState(a.name);
  const [message, setMessage] = useState<string | null>(null);
  const finished = FINISHED.includes(a.status) || a.status === "ready";
  const t = tone(a.status);

  const run = async (f: () => Promise<unknown>, ok?: string) => {
    setMessage(null);
    try {
      await f();
      if (ok) setMessage(ok);
    } catch (e) {
      setMessage(String(e));
    }
  };

  const exportMd = async () => {
    const path = await save({ defaultPath: `${a.name}.md`, filters: [{ name: "Markdown", extensions: ["md"] }] });
    if (path) run(() => api.agentExport(a.id, path), "Saved.");
  };

  return (
    <article className={`detail detail--${t}`}>
      <header className="detail__head">
        <div className="detail__title">
          {renaming ? (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                run(() => api.agentRename(a.id, name)).then(() => setRenaming(false));
              }}
            >
              <input className="rename" value={name} onChange={(e) => setName(e.target.value)} autoFocus onBlur={() => setRenaming(false)} />
            </form>
          ) : (
            <h1 onDoubleClick={() => setRenaming(true)} title="Double-click to rename">
              {a.name}
            </h1>
          )}
          <span className="pill">{STATUS_LABEL[a.status]}</span>
        </div>
        <div className="detail__actions">
          {(a.status === "running" || a.status === "queued" || a.status === "approval" || a.status === "question") && (
            <button type="button" className="btn" onClick={() => run(() => api.agentPause(a.id))}>
              Pause
            </button>
          )}
          {a.status === "paused" && (
            <button type="button" className="btn btn--primary" onClick={() => run(() => api.agentResume(a.id))}>
              Resume
            </button>
          )}
          {!finished && (
            <button type="button" className="btn" onClick={() => run(() => api.agentCancel(a.id))}>
              Cancel
            </button>
          )}
          {["failed", "stopped", "cancelled"].includes(a.status) && (
            <button type="button" className="btn btn--primary" onClick={() => run(() => api.agentRetry(a.id))}>
              Retry
            </button>
          )}
          <button type="button" className="btn" onClick={() => run(() => api.agentDuplicate(a.id), "Started a copy.")}>
            Run again as new
          </button>
          {!a.parent && (
            <button
              type="button"
              className="btn btn--quiet"
              onClick={() => run(async () => { await api.saveTemplate(a.id); }, "Saved as a template. Add blanks to it in Settings → Agents → Templates.")}
            >
              Save as template
            </button>
          )}
          {finished && (
            <button type="button" className="btn btn--quiet" onClick={() => run(() => api.agentDelete(a.id))}>
              Remove
            </button>
          )}
        </div>
      </header>

      <p className="detail__goal">{a.goal}</p>
      {(a.after.length > 0 || a.parent) && (
        <p className="muted small">
          {a.parent && `Helper of ${agentName(a.parent)}. `}
          {a.after.length > 0 && `Started after ${a.after.map(agentName).join(" and ")}, with their results.`}
        </p>
      )}
      {batch && batch.request !== a.goal && <p className="muted small">Asked for: “{batch.request}”</p>}
      {message && <p className="note">{message}</p>}

      {!finished && a.status !== "approval" && a.status !== "question" && (
        <p className="detail__now">
          <span className={`dot dot--${t}`} aria-hidden="true" />
          {line ?? (a.statusLine || "Waiting to start.")}
        </p>
      )}

      {a.pending?.type === "approval" && <ApprovalBox key={a.pending.id} agent={a} pending={a.pending} />}

      {a.pending?.type === "question" && (
        <section className="box box--question">
          <h2>{a.name} asks</h2>
          <p className="box__summary">{a.pending.question}</p>
          <div className="box__buttons">
            {a.pending.options.map((o) => (
              <button key={o} type="button" className="btn" onClick={() => run(() => api.agentAnswer(a.id, { type: "choice", text: o }))}>
                {o}
              </button>
            ))}
          </div>
          <form
            className="inline"
            onSubmit={(e) => {
              e.preventDefault();
              if (answer.trim()) run(() => api.agentAnswer(a.id, { type: "choice", text: answer.trim() }));
            }}
          >
            <input className="field" value={answer} onChange={(e) => setAnswer(e.target.value)} placeholder="Type an answer…" />
            <button type="submit" className="btn btn--primary" disabled={!answer.trim()}>
              Answer
            </button>
          </form>
        </section>
      )}

      {a.pending?.type === "failure" && (
        <section className="box box--question">
          <h2>A step failed</h2>
          <p className="box__summary">{a.pending.message}</p>
          <div className="box__buttons">
            <button type="button" className="btn btn--primary" onClick={() => run(() => api.agentAnswer(a.id, { type: "retry" }))}>
              Retry
            </button>
            <button type="button" className="btn" onClick={() => run(() => api.agentAnswer(a.id, { type: "skip" }))}>
              Skip this step
            </button>
            <button type="button" className="btn btn--quiet" onClick={() => run(() => api.agentAnswer(a.id, { type: "cancel" }))}>
              Cancel the agent
            </button>
          </div>
        </section>
      )}

      {(a.stop || a.error) && (
        <section className="box box--alert">
          <h2>{a.stop ? "Stopped at a limit" : "Failed"}</h2>
          <p className="box__summary">{a.stop?.message ?? a.error}</p>
          {a.stop && (a.stop.limit === "agentBudget" || a.stop.limit === "batchBudget") && a.counters.extraTokens === 0 && (
            <div className="box__buttons">
              <button type="button" className="btn btn--primary" onClick={() => run(() => api.agentRaise(a.id))}>
                Raise the limit and continue once
              </button>
            </div>
          )}
        </section>
      )}

      {a.result && (
        <section className="result">
          <div className="result__head">
            <h2>Result</h2>
            <button type="button" className="btn btn--quiet" onClick={() => run(() => navigator.clipboard.writeText(a.result ?? ""), "Copied.")}>
              Copy
            </button>
            <button type="button" className="btn btn--quiet" onClick={exportMd}>
              Export Markdown
            </button>
          </div>
          <AgentFiles agent={a.id} files={a.files} />
          <div className="result__body">
            <Markdown components={{ a: ({ children, href }) => <span title={href}>{children}</span>, img: () => null }}>{a.result}</Markdown>
          </div>
          {a.suggestions.length > 0 && (
            <div className="suggest">
              {a.suggestions.map((s) => (
                <button key={s} type="button" className="chip" onClick={() => run(() => api.agentFollowUp(a.id, s))}>
                  {s}
                </button>
              ))}
            </div>
          )}
        </section>
      )}

      {["done", "ready", "running", "queued", "paused"].includes(a.status) && (
        <form
          className="followup"
          onSubmit={(e) => {
            e.preventDefault();
            const done = a.status === "done" || a.status === "ready";
            run(() => (done ? api.agentFollowUp(a.id, followUp) : api.agentSteer(a.id, followUp)), done ? undefined : "It'll hear that at its next step.").then(() => setFollowUp(""));
          }}
        >
          <input value={followUp} onChange={(e) => setFollowUp(e.target.value)} placeholder={a.status === "done" || a.status === "ready" ? (a.keepOpen ? "What should change?" : "Ask this agent for more…") : "Tell it something while it works…"} />
          <button type="submit" className="sendbtn" aria-label="Send" disabled={!followUp.trim()}>
            <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true">
              <path d="M10 15.5v-11M5.5 9 10 4.5 14.5 9" />
            </svg>
          </button>
        </form>
      )}

      <div className="statsrow">
        <dl className="stats">
          <Stat label="Steps" value={`${a.counters.steps}/${a.maxSteps}`} />
          <Stat label="Tool calls" value={String(a.counters.toolCalls)} />
          <Stat label="Tokens" value={tokens(a.counters.tokens)} />
          <Stat label="Cost" value={a.counters.costKnown ? `$${a.counters.cost.toFixed(3)}` : "Unknown"} />
          <Stat label="Time" value={duration(a.activeMs)} />
          {a.changes > 0 && <Stat label="File changes" value={String(a.changes)} />}
        </dl>
        {a.changes > 0 && finished && (
          <button
            type="button"
            className="btn btn--soft"
            onClick={() => run(async () => {
              const problems = await api.agentUndo(a.id);
              if (problems.length) throw new Error(`Some changes couldn't be undone: ${problems.join("; ")}`);
            }, "Every file change was put back.")}
          >
            Undo all {a.changes} file changes
          </button>
        )}
      </div>

      <section className="timeline">
        <h2>What it did</h2>
        <ol>
          {[...a.log].reverse().map((l, i) => (
            <li key={a.log.length - i} className={`tl tl--${l.kind}`}>
              <i className="tl__dot" aria-hidden="true" />
              <time>{new Date(l.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</time>
              <span className="tl__what">
                <span className="tl__kind">{KIND[l.kind]}</span> {l.kind === "tool" ? <code>{l.text}</code> : l.text}
              </span>
            </li>
          ))}
        </ol>
      </section>
    </article>
  );
}

const KIND: Record<LogKind, string> = {
  status: "Said",
  tool: "Ran",
  result: "Got",
  retry: "Retry",
  approval: "You",
  error: "Problem",
  note: "Note",
};

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function firstLine(t: string) {
  return t.split("\n")[0];
}
