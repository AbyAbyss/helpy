import { useState } from "react";
import Markdown from "react-markdown";
import type { AgentView } from "../bindings/AgentView";
import { AttachButton, AttachmentStrip, useAttachments } from "../lib/attachments";
import { api } from "../lib/ipc";
import { AgentFiles } from "./Files";
import { duration, lastCommand, spend, STATUS_LABEL, tone } from "./dockState";

const FINISHED = ["done", "ready", "failed", "stopped", "cancelled"];

/**
 * One agent's card. With native glass (macOS, Windows 11) the follow-up bar
 * sits inside the card, so the whole window is one rounded glass shape;
 * otherwise it floats under the card (the split view).
 */
export function Card(props: {
  agent: AgentView;
  line: string | undefined;
  merged: boolean;
  onPin: (on: boolean) => void;
  onClose: () => void;
}) {
  const { agent: a, line } = props;
  const t = tone(a.status);
  const cmd = lastCommand(a);
  const working = a.status === "running" || a.status === "queued" || a.status === "paused";
  const progress = Math.min(1, a.counters.steps / Math.max(1, a.maxSteps));
  const done = (a.status === "done" || a.status === "ready") && !!a.result;
  const bar = done ? (
    <FollowBar
      agent={a}
      inset={props.merged}
      placeholder={a.keepOpen ? "What should change?" : "Ask a follow-up…"}
      pictures
      onSend={(text, image) => api.agentFollowUp(a.id, text, image)}
      onVoice={() => api.agentVoiceFollowUp(a.id)}
      onPin={props.onPin}
    />
  ) : working ? (
    <FollowBar
      agent={a}
      inset={props.merged}
      placeholder="Tell it something…"
      onSend={(text) => api.agentSteer(a.id, text)}
      onVoice={() => api.agentVoiceFollowUp(a.id)}
      onPin={props.onPin}
    />
  ) : a.pending?.type === "question" ? (
    <FollowBar
      agent={a}
      inset={props.merged}
      placeholder="Or type an answer…"
      onSend={(text) => api.agentAnswer(a.id, { type: "choice", text })}
      onPin={props.onPin}
    />
  ) : null;

  return (
    <div className={`stack${props.merged ? " stack--merged" : ""}`}>
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
            <AgentFiles agent={a.id} files={a.files} compact />
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

        {props.merged && bar}

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

      {!props.merged && bar}
    </div>
  );
}

/** The split view's second piece: a follow-up bar floating under the card. */
function FollowBar(props: {
  agent: AgentView;
  inset: boolean;
  placeholder: string;
  /** Pictures can go with the message (follow-ups to finished agents). */
  pictures?: boolean;
  onSend: (text: string, image?: string) => void;
  onVoice?: () => void;
  onPin: (on: boolean) => void;
}) {
  const { agent: a, onPin } = props;
  const [text, setText] = useState("");
  const [sent, setSent] = useState(0);
  const attach = useAttachments(1);
  const send = () => {
    if (!text.trim()) return;
    props.onSend(text.trim(), attach.images[0]);
    setText("");
    attach.clear();
    setSent((n) => n + 1);
    onPin(false);
  };
  return (
    <>
      <AttachmentStrip images={attach.images} onRemove={attach.remove} />
      <form
        className={`followbar card--${tone(a.status)}${props.inset ? " followbar--inset" : ""}`}
        onSubmit={(e) => {
          e.preventDefault();
          send();
        }}
      >
        {props.pictures && <AttachButton className="round" onOpen={() => onPin(true)} onFiles={(f) => attach.add(f)} />}
        <input
          value={text}
          onChange={(e) => setText(e.target.value)}
          onPaste={props.pictures ? attach.onPaste : undefined}
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
    </>
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
