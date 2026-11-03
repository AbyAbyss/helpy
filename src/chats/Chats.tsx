import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Chat } from "../bindings/Chat";
import type { ChatSummary } from "../bindings/ChatSummary";
import { useLook } from "../agents/Panel";
import { api, EVENTS } from "../lib/ipc";
import { RichText } from "../lib/markdown";
import { useSettings, useTheme } from "../lib/useSettings";
import { dayGroup, matches, type Kind } from "./group";

const KINDS: { value: Kind; label: string }[] = [
  { value: "all", label: "All" },
  { value: "typed", label: "Typed" },
  { value: "voice", label: "Voice" },
];

/** Every past conversation with Helpy, typed or spoken, like the agent panel. */
export function Chats() {
  const [settings] = useSettings();
  useTheme(settings);
  useLook(settings?.general.theme);
  const [list, setList] = useState<ChatSummary[] | null>(null);
  const [kind, setKind] = useState<Kind>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [chat, setChat] = useState<Chat | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    const root = document.documentElement;
    if (navigator.platform.includes("Mac")) root.setAttribute("data-mac", "");
    api.windowGlass().then((g) => g && root.setAttribute("data-glass", g), () => {});
  }, []);

  // New questions and deletions anywhere reload the list and the open chat.
  useEffect(() => {
    const changed = listen(EVENTS.chatsChanged, () => setVersion((v) => v + 1));
    const focus = listen<string>(EVENTS.chatsFocus, (e) => setSelected(e.payload));
    return () => {
      void changed.then((f) => f());
      void focus.then((f) => f());
    };
  }, []);

  useEffect(() => void api.chats().then(setList, (e) => setError(String(e))), [version]);
  useEffect(() => {
    if (!selected) return setChat(null);
    api.chat(selected).then(setChat, (e) => setError(String(e)));
  }, [selected, version]);

  const groups = useMemo(() => {
    const out = new Map<string, ChatSummary[]>();
    const now = new Date();
    for (const c of (list ?? []).filter((c) => matches(c, kind, query))) {
      const g = dayGroup(c.updated, now);
      out.set(g, [...(out.get(g) ?? []), c]);
    }
    return [...out.entries()];
  }, [list, kind, query]);

  const shown = groups.reduce((n, [, l]) => n + l.length, 0);

  return (
    <div className="ap">
      <aside className="ap__list">
        <header className="ap__head" data-tauri-drag-region>
          <h1>Chats</h1>
          <div className="seg" role="radiogroup" aria-label="Show">
            {KINDS.map((k) => (
              <button key={k.value} type="button" role="radio" aria-checked={kind === k.value} onClick={() => setKind(k.value)}>
                {k.label}
              </button>
            ))}
          </div>
        </header>
        <div className="chats__search">
          <input type="search" value={query} placeholder="Search your chats" aria-label="Search chats" onChange={(e) => setQuery(e.target.value)} />
        </div>
        <div className="ap__groups">
          {list && shown === 0 && (
            <p className="ap__empty">
              {list.length === 0 ? "No chats yet. Everything you ask Helpy, typed or out loud, shows up here." : "No chats match."}
            </p>
          )}
          {groups.map(([title, items]) => (
            <section key={title} className="group">
              <h2 className="group__title">
                <span>{title}</span>
              </h2>
              {items.map((c) => (
                <button key={c.id} type="button" className={`row${selected === c.id ? " is-on" : ""}`} onClick={() => setSelected(c.id)}>
                  <span className={`chats__icon${c.voice ? " chats__icon--voice" : ""}`} aria-label={c.allVoice ? "Spoken" : c.voice ? "Typed and spoken" : "Typed"}>
                    {c.voice ? <MicIcon /> : <TypeIcon />}
                  </span>
                  <span className="row__main">
                    <span className="row__name chats__title">{c.title || "Untitled"}</span>
                    <span className="row__line">{c.preview}</span>
                  </span>
                  <span className="row__status">{when(c.updated)}</span>
                </button>
              ))}
            </section>
          ))}
        </div>
        {list && list.length > 0 && <ClearAll onError={setError} />}
      </aside>
      <main className="ap__detail">
        {error && <p className="err-text chats__error">{error}</p>}
        {chat ? (
          <Detail key={chat.id} chat={chat} onError={setError} onDeleted={() => setSelected(null)} />
        ) : (
          <div className="ap__placeholder">
            <p>Pick a chat to read it again, or carry on where you left off.</p>
            <p className="muted">Questions you ask out loud are kept here too, marked with a microphone.</p>
          </div>
        )}
      </main>
    </div>
  );
}

function Detail({ chat, onError, onDeleted }: { chat: Chat; onError: (m: string) => void; onDeleted: () => void }) {
  const [copied, setCopied] = useState(false);
  const spoken = chat.turns.filter((t) => t.voice).length;
  const run = (f: () => Promise<unknown>) => f().catch((e) => onError(String(e)));
  const copy = async () => {
    const text = chat.turns.map((t) => `You: ${t.question}\n\nHelpy: ${t.answer || t.error || ""}`).join("\n\n---\n\n");
    await navigator.clipboard.writeText(`${chat.title}\n\n${text}`);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  return (
    <article className="detail chats__detail">
      <header className="detail__head">
        <div className="detail__title">
          <h1>{chat.title || "Untitled"}</h1>
        </div>
        <div className="detail__actions">
          <button type="button" className="btn btn--primary" onClick={() => run(() => api.chatContinue(chat.id))} title="Opens it in the chat panel; your next question carries on from here">
            Continue chat
          </button>
          <button type="button" className="btn" onClick={() => run(copy)}>
            {copied ? "Copied" : "Copy"}
          </button>
          <button type="button" className="btn" onClick={() => run(() => api.chatDelete(chat.id).then(onDeleted))}>
            Delete
          </button>
        </div>
      </header>
      <p className="detail__goal chats__meta">
        {new Date(chat.started).toLocaleString(undefined, { dateStyle: "full", timeStyle: "short" })} · {chat.turns.length} {chat.turns.length === 1 ? "question" : "questions"}
        {spoken > 0 && ` · ${spoken === chat.turns.length ? "all" : spoken} spoken`}
      </p>
      <div className="chats__thread">
        {chat.turns.map((t, i) => (
          <div key={i} className="chats__turn">
            <div className={`chats__q${t.voice ? " chats__q--voice" : ""}`}>
              {t.voice && <MicIcon />}
              <span>{t.question}</span>
            </div>
            {(t.images > 0 || t.documents.length > 0) && (
              <div className="chats__att muted small">
                {[t.images > 0 && `${t.images} ${t.images === 1 ? "picture" : "pictures"}`, ...t.documents].filter(Boolean).join(" · ")}
              </div>
            )}
            {t.answer.trim() && (
              <div className="chats__a">
                <RichText text={t.answer} />
              </div>
            )}
            {t.error && <div className={t.error === "Stopped." ? "chats__stopped muted small" : "chats__err err-text"}>{t.error}</div>}
            <div className="chats__time muted small">
              {new Date(t.at).toLocaleTimeString(undefined, { timeStyle: "short" })}
              {t.model && ` · ${t.model}`}
            </div>
          </div>
        ))}
      </div>
    </article>
  );
}

/** Deletes every chat, after a second click to be sure. */
function ClearAll({ onError }: { onError: (m: string) => void }) {
  const [sure, setSure] = useState(false);
  useEffect(() => {
    if (!sure) return;
    const t = setTimeout(() => setSure(false), 4000);
    return () => clearTimeout(t);
  }, [sure]);
  return (
    <footer className="chats__foot">
      <button
        type="button"
        className={`link-btn${sure ? " chats__danger" : ""}`}
        onClick={() => {
          if (!sure) return setSure(true);
          setSure(false);
          api.chatsClear().catch((e) => onError(String(e)));
        }}
      >
        {sure ? "Click again to delete every chat" : "Delete all chats…"}
      </button>
    </footer>
  );
}

function when(ms: number) {
  const d = new Date(ms);
  const today = new Date().toDateString() === d.toDateString();
  return today ? d.toLocaleTimeString(undefined, { timeStyle: "short" }) : d.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

function MicIcon() {
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
      <rect x="5.5" y="1.5" width="5" height="8.5" rx="2.5" />
      <path d="M3 8a5 5 0 0 0 10 0M8 13v1.5" />
    </svg>
  );
}

function TypeIcon() {
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
      <path d="M2.5 4h11a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1zM4.5 7h.01M7 7h.01M9.5 7h.01M12 7h.01M5 9.5h6" />
    </svg>
  );
}
