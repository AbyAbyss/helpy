import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import Markdown from "react-markdown";
import type { AskAction } from "../bindings/AskAction";
import type { AskEvent } from "../bindings/AskEvent";
import type { AskStatus } from "../bindings/AskStatus";
import { AttachButton, AttachmentStrip, useAttachments } from "../lib/attachments";
import { api, EVENTS } from "../lib/ipc";
import { linkify, WebLink } from "../lib/links";
import { useSettings, useTheme } from "../lib/useSettings";
import { apply, waiting, type Item } from "./transcript";

const EXAMPLES = ["How do I find my spam folder?", "What does this error mean?", "Summarize what's on my screen"];

export function Panel() {
  const [settings] = useSettings();
  useTheme(settings);
  const [status, setStatus] = useState<AskStatus | null>(null);
  const [items, setItems] = useState<Item[]>([]);
  const [running, setRunning] = useState(false);
  const [meta, setMeta] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const attach = useAttachments(4);
  const input = useRef<HTMLTextAreaElement>(null);
  const scroller = useRef<HTMLDivElement>(null);

  const refreshStatus = useCallback(() => void api.askStatus().then(setStatus), []);
  useEffect(refreshStatus, [settings, refreshStatus]);

  useEffect(() => {
    const off = listen(EVENTS.askShown, () => {
      refreshStatus();
      input.current?.focus();
    });
    input.current?.focus();
    return () => void off.then((f) => f());
  }, [refreshStatus]);

  // Questions can come from this panel or from voice; both arrive as events.
  useEffect(() => {
    const off = listen<AskEvent>(EVENTS.ask, ({ payload: e }) => {
      setItems((prev) => apply(prev, e));
      if (e.type === "question") {
        setRunning(true);
        setMeta(null);
      }
      if (e.type === "started") setMeta(e.model);
      if (e.type === "done") setMeta(`${e.model} · ${e.tokens.toLocaleString()} tokens`);
      if (e.type === "done" || e.type === "error") setRunning(false);
    });
    return () => void off.then((f) => f());
  }, []);

  // Follow the answer as it streams, unless the user scrolled up to read.
  useEffect(() => {
    const el = scroller.current;
    if (el && el.scrollHeight - el.scrollTop - el.clientHeight < 120) el.scrollTop = el.scrollHeight;
  }, [items]);

  const send = async (text: string) => {
    const images = attach.images;
    const q = text.trim() || (images.length ? "Look at this." : "");
    if (!q) return;
    setDraft("");
    attach.clear();
    try {
      await api.ask(q, images);
    } catch (e) {
      setItems((prev) => [...prev, { kind: "error", text: String(e), action: null }]);
    } finally {
      input.current?.focus();
    }
  };

  const dismiss = () => {
    api.askReset();
    api.askHide();
    setItems([]);
    setMeta(null);
    setDraft("");
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      if (running) api.askCancel();
      else dismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const startAgents = async (text: string) => {
    setItems((prev) => prev.filter((i) => i.kind !== "offer"));
    try {
      await api.plan(text);
    } catch (e) {
      setItems((prev) => [...prev, { kind: "error", text: String(e), action: null }]);
    }
  };

  const answerPermission = (id: number, allow: boolean) => {
    api.askScreenAnswer(id, allow);
    setItems((prev) => prev.map((i) => (i.kind === "permission" && i.id === id ? { ...i, answer: allow ? "allowed" : "denied" } : i)));
  };

  const ready = status && !status.problem;
  const waitingCount = items.filter((i) => i.kind === "user" && i.queued).length;

  return (
    <div className="panel-card">
      <header className="ph" data-tauri-drag-region>
        <PipMark />
        <span className="ph__name" data-tauri-drag-region>Helpy</span>
        {status?.model && (
          <span className="ph__model mono" title={status.model} data-tauri-drag-region>
            {status.model}
          </span>
        )}
        <span className="ph__spacer" data-tauri-drag-region />
        {waitingCount > 0 && (
          <span className="ph__waiting" title="Asked after the current answer" data-tauri-drag-region>
            {waitingCount} waiting
          </span>
        )}
        {ready && <ScreenChip note={status.screenNote} mode={settings?.answerStyle.screenAccess} />}
        <button type="button" className="ph__close" aria-label="Close (Esc)" title="Close (Esc)" onClick={dismiss}>
          <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true"><path d="M2 2l8 8M10 2l-8 8" /></svg>
        </button>
      </header>

      <div className="thread" ref={scroller} aria-live="polite">
        {status?.problem ? (
          <Setup problem={status.problem} />
        ) : items.length === 0 ? (
          <Welcome onPick={(q) => { setDraft(q); input.current?.focus(); }} />
        ) : (
          items.map((item, i) => <ItemView key={i} item={item} onPermission={answerPermission} onAgents={startAgents} />)
        )}
        {waiting(items, running) && (
          <div className="dots" aria-label="Thinking">
            <i /> <i /> <i />
          </div>
        )}
      </div>

      <footer className="composer">
        <AttachmentStrip images={attach.images} onRemove={attach.remove} />
        <div className="composer__box">
          <AttachButton onFiles={(f) => attach.add(f)} disabled={!ready} />
          <textarea
            ref={input}
            id="ask-input"
            rows={1}
            placeholder={ready ? "Ask anything about what you're doing…" : "Set up a model first"}
            value={draft}
            disabled={!ready}
            onChange={(e) => {
              setDraft(e.target.value);
              e.target.style.height = "auto";
              e.target.style.height = `${Math.min(e.target.scrollHeight, 120)}px`;
            }}
            onPaste={attach.onPaste}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                send(draft);
              }
            }}
          />
          {running && !draft.trim() && !attach.images.length ? (
            <button type="button" className="send send--stop" aria-label="Stop" title="Stop (Esc)" onClick={() => api.askCancel()}>
              <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true"><rect x="1.5" y="1.5" width="9" height="9" rx="1.5" /></svg>
            </button>
          ) : (
            <button type="button" className="send" aria-label={running ? "Ask next" : "Send"} title={running ? "Asked after this answer (Enter)" : "Send (Enter)"} disabled={(!draft.trim() && !attach.images.length) || !ready} onClick={() => send(draft)}>
              <svg viewBox="0 0 14 14" width="14" height="14" aria-hidden="true"><path d="M7 12V2M7 2 2.5 6.5M7 2l4.5 4.5" /></svg>
            </button>
          )}
        </div>
        <div className="composer__meta">
          {attach.error ? (
            <span className="attach-error">{attach.error}</span>
          ) : (
            <span>{running ? "Still answering · a new question is asked next, without stopping this one · Esc to stop" : (meta ?? "Enter to send · Shift+Enter for a new line · Paste a picture · Esc to close")}</span>
          )}
        </div>
      </footer>
    </div>
  );
}

function ItemView({ item, onPermission, onAgents }: { item: Item; onPermission: (id: number, allow: boolean) => void; onAgents: (text: string) => void }) {
  const [big, setBig] = useState(false);
  switch (item.kind) {
    case "user":
      return (
        <div className={`msg msg--user${item.voice ? " msg--voice" : ""}${item.queued ? " msg--queued" : ""}`} title={item.queued ? "Asked after the current answer" : undefined}>
          {item.images && <AttachmentStrip images={item.images} />}
          {item.voice && <MicIcon />}
          {item.text}
          {item.queued && <span className="msg__queued">next</span>}
        </div>
      );
    case "offer":
      return (
        <div className="offer">
          <span>Want Helpy to do this instead?</span>
          <button type="button" className="offer__btn" onClick={() => onAgents(item.text)}>
            Do it with agents
          </button>
        </div>
      );
    case "assistant":
      return (
        <div className="msg msg--ai">
          <Markdown
            components={{
              a: WebLink,
              img: () => null,
            }}
          >
            {linkify(item.text)}
          </Markdown>
        </div>
      );
    case "screen":
      return (
        <button type="button" className={`shot${big ? " is-big" : ""}`} onClick={() => setBig(!big)} title={big ? "Make smaller" : "Show larger"}>
          <img src={item.thumbnail} alt={`What Helpy saw on ${item.monitor}`} />
          <span>Looked at {item.monitor}</span>
        </button>
      );
    case "step":
      return (
        <div className="step-item">
          <span className="step-item__num" aria-hidden="true">
            {item.number}
          </span>
          <span>
            <span className="step-item__of">
              Step {item.number}
              {item.total != null && ` of ${item.total}`}
            </span>
            {item.text}
          </span>
        </div>
      );
    case "permission":
      if (item.answer) return <div className="note">{item.answer === "allowed" ? "You let Helpy look at your screen." : "You kept your screen private."}</div>;
      return (
        <div className="ask-perm" role="alertdialog" aria-label="Screen permission">
          <p>Helpy wants to look at your screen to answer this.</p>
          <div className="ask-perm__actions">
            <button type="button" className="btn-s btn-s--primary" onClick={() => onPermission(item.id, true)} autoFocus>
              Allow once
            </button>
            <button type="button" className="btn-s" onClick={() => onPermission(item.id, false)}>
              Not now
            </button>
          </div>
        </div>
      );
    case "retry":
      return (
        <div className="note note--retry">
          <span className="spin" aria-hidden="true" />
          {item.text}
        </div>
      );
    case "notice":
      return <div className="note">{item.text}</div>;
    case "error":
      return (
        <div className="err" role="alert">
          <p>{item.text}</p>
          {item.action && <ActionButton action={item.action} />}
        </div>
      );
  }
}

function ActionButton({ action }: { action: AskAction }) {
  const label = action === "openLimits" ? "Open limits" : "Open AI providers";
  return (
    <button type="button" className="btn-s" onClick={() => api.openSettingsSection(action === "openLimits" ? "usage" : "ai")}>
      {label}
    </button>
  );
}

function Setup({ problem }: { problem: string }) {
  return (
    <div className="setup">
      <PipMark size={40} />
      <h2>Helpy needs an AI model</h2>
      <p>{problem}</p>
      <p className="muted">Use a cloud provider with an API key, or a free model running on this computer with Ollama or LM Studio.</p>
      <button type="button" className="btn-s btn-s--primary" onClick={() => api.openSettingsSection("ai")}>
        Open AI providers
      </button>
    </div>
  );
}

function Welcome({ onPick }: { onPick: (q: string) => void }) {
  return (
    <div className="welcome">
      <h2>What can I help with?</h2>
      <p className="muted">Ask about the app in front of you. Follow-up questions keep the context until you close this panel.</p>
      <div className="examples">
        {EXAMPLES.map((q) => (
          <button key={q} type="button" className="example" onClick={() => onPick(q)}>
            {q}
          </button>
        ))}
      </div>
    </div>
  );
}

function ScreenChip({ note, mode }: { note: string | null; mode?: string }) {
  const off = !!note;
  const label = off ? note : mode === "always" ? "Sees your screen with every question" : mode === "ask" ? "Asks before looking at your screen" : "Looks at your screen when a question needs it";
  return (
    <span className={`schip${off ? " schip--off" : ""}`} title={label} aria-label={label}>
      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
        <path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8Z" />
        <circle cx="8" cy="8" r="2" />
        {off && <path d="M2.5 13.5l11-11" />}
      </svg>
    </span>
  );
}

function PipMark({ size = 20 }: { size?: number }) {
  return (
    <svg viewBox="0 0 64 64" width={size} height={size} aria-hidden="true" className="pip">
      <path d="M6 6 L26 13.5 A22 22 0 1 1 13.5 26 Z" fill="var(--accent)" />
      <ellipse cx="29" cy="33" rx="3.2" ry="4.4" fill="var(--surface)" />
      <ellipse cx="41" cy="33" rx="3.2" ry="4.4" fill="var(--surface)" />
    </svg>
  );
}

function MicIcon() {
  return (
    <svg className="msg__mic" viewBox="0 0 16 16" width="12" height="12" aria-label="Spoken">
      <rect x="5.5" y="1.5" width="5" height="8.5" rx="2.5" />
      <path d="M3 7.5a5 5 0 0 0 10 0M8 12.5v2" />
    </svg>
  );
}
