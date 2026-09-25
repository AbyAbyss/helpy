import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { AgentTools } from "../bindings/AgentTools";
import type { BrowserInfo } from "../bindings/BrowserInfo";
import type { Builder } from "../bindings/Builder";
import type { CoderInfo } from "../bindings/CoderInfo";
import type { Coder } from "../bindings/Coder";
import type { Approvals } from "../bindings/Approvals";
import type { Rule } from "../bindings/Rule";
import type { SearchEngine } from "../bindings/SearchEngine";
import { api } from "../lib/ipc";
import { Segmented, Select, Toggle } from "./controls";

const RULES = [
  { value: "allow", label: "Allow" },
  { value: "ask", label: "Ask me" },
  { value: "never", label: "Never" },
];

/** Allow / Ask / Never for each kind of action agents take. */
export function ApprovalRules({ value, onChange }: { value: Approvals; onChange: (v: Approvals) => void }) {
  const rows: { key: keyof Approvals; label: string; help: string }[] = [
    { key: "fileChanges", label: "Changing files", help: "Writing, moving and renaming. Backed up and undoable." },
    { key: "fileDeletes", label: "Deleting files", help: "A backup is kept either way." },
    { key: "reminders", label: "Reminders and events", help: "Adding them to your reminders or calendar." },
    { key: "browserForms", label: "Typing into websites", help: "Filling in and sending forms in the agents' own browser." },
    { key: "builds", label: "Building apps", help: "Each coding round, command and launch of a builder agent, in its own project folder." },
  ];
  return (
    <div className="rules">
      {rows.map((r) => (
        <div key={r.key} className="rules__row">
          <div>
            <div className="rules__label">{r.label}</div>
            <div className="rules__help">{r.help}</div>
          </div>
          <Segmented id={`rule-${r.key}`} value={value[r.key]} options={RULES} onChange={(v) => onChange({ ...value, [r.key]: v as Rule })} />
        </div>
      ))}
    </div>
  );
}

export function ToolToggles({ value, onChange }: { value: AgentTools; onChange: (v: AgentTools) => void }) {
  const rows: { key: keyof AgentTools; label: string }[] = [
    { key: "webSearch", label: "Web search" },
    { key: "fetch", label: "Reading web pages" },
    { key: "files", label: "Files in approved folders" },
    { key: "shell", label: "Shell commands" },
    { key: "reminders", label: "Reminders and calendar" },
    { key: "browser", label: "Web browser (for pages that need JavaScript, and scraping)" },
    { key: "build", label: "Building apps and sites" },
  ];
  return (
    <div className="toggles">
      {rows.map((r) => (
        <label key={r.key} className="toggles__row">
          <span>{r.label}</span>
          <Toggle id={`tool-${r.key}`} checked={value[r.key]} onChange={(v) => onChange({ ...value, [r.key]: v })} />
        </label>
      ))}
      {value.browser && <BrowserStatus />}
    </div>
  );
}

/** Which browser agents use, with a download when there's none. */
function BrowserStatus() {
  const [info, setInfo] = useState<BrowserInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => void api.browserInfo().then(setInfo, () => {}), []);
  if (!info) return null;
  const download = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.browserDownload();
      setInfo(await api.browserInfo());
    } catch (e) {
      setError(String(e));
    }
    setBusy(false);
  };
  return (
    <div className="engine__msg">
      {info.path ? (
        <>Agents use the browser at <code>{info.path}</code>, with Helpy's own profile (your logins and history aren't touched).</>
      ) : (
        <>
          No Chrome, Edge, Chromium or Brave found.{" "}
          <button type="button" className="link-btn" onClick={download} disabled={busy}>
            {busy ? "Downloading…" : "Download Chromium for agents (about 150 MB)"}
          </button>
        </>
      )}
      {error && <span className="err-text"> {error}</span>}
    </div>
  );
}

/** The privacy blocklist, with a note when this desktop can't list windows. */
export function Blocklist({ value, onChange }: { value: string[]; onChange: (v: string[]) => void }) {
  const [ok, setOk] = useState(true);
  useEffect(() => void api.canSeeWindows().then(setOk, () => setOk(false)), []);
  return (
    <>
      <StringList value={value} placeholder="Add an app or a title word, e.g. bank" onChange={onChange} />
      {!ok && (
        <div className="engine__msg err-text">
          This desktop doesn't tell Helpy which windows are open (as on Wayland), so the list can't be checked here. Pause screen capture before opening something private.
        </div>
      )}
    </>
  );
}

const BUILDERS: { value: Builder; label: string; tool?: Coder }[] = [
  { value: "auto", label: "Automatic" },
  { value: "claudeCode", label: "Claude Code", tool: "claudeCode" },
  { value: "codex", label: "OpenAI Codex", tool: "codex" },
  { value: "openCode", label: "opencode", tool: "openCode" },
  { value: "custom", label: "My own command" },
  { value: "helpy", label: "Helpy itself" },
];

/** The coding tool builder agents use, marked installed or not. */
export function BuilderPicker({ value, onChange }: { value: Builder; onChange: (v: Builder) => void }) {
  const [found, setFound] = useState<CoderInfo[] | null>(null);
  useEffect(() => void api.builders().then(setFound, () => setFound([])), []);
  const has = (t?: Coder) => !!found?.find((f) => f.tool === t)?.path;
  const chosen = BUILDERS.find((b) => b.value === value);
  const auto = found && BUILDERS.find((b) => b.tool && has(b.tool));
  return (
    <div className="builder">
      <Select
        id="agents.builder"
        value={value}
        options={BUILDERS.map((b) => ({
          value: b.value,
          label: b.label + (b.tool && found ? (has(b.tool) ? " (installed)" : " (not found)") : ""),
        }))}
        onChange={(v) => onChange(v as Builder)}
      />
      {found && (
        <div className="engine__msg">
          {value === "auto" && (auto ? <>Using {auto.label}.</> : <>No coding tool found, so Helpy writes the code itself.</>)}
          {chosen?.tool && !has(chosen.tool) && <>{chosen.label} isn't installed or isn't on your PATH, so builder agents can't use it.</>}
        </div>
      )}
    </div>
  );
}

async function pickFolder(): Promise<string | null> {
  const path = await open({ directory: true, multiple: false });
  return typeof path === "string" ? path : null;
}

export function FolderList({ value, onChange }: { value: string[]; onChange: (v: string[]) => void }) {
  return (
    <div className="paths">
      {value.length === 0 && <p className="paths__empty">None yet. Agents ask for a folder in their plan, or add one here.</p>}
      {value.map((f) => (
        <div key={f} className="paths__row">
          <code>{f}</code>
          <button type="button" className="link-btn" onClick={() => onChange(value.filter((x) => x !== f))}>
            Remove
          </button>
        </div>
      ))}
      <button
        type="button"
        className="btn btn--sm"
        onClick={async () => {
          const f = await pickFolder();
          if (f && !value.includes(f)) onChange([...value, f]);
        }}
      >
        Add folder…
      </button>
    </div>
  );
}

export function FolderPicker({ value, placeholder, onChange }: { value: string; placeholder: string; onChange: (v: string) => void }) {
  return (
    <div className="paths paths--one">
      <code className={value ? undefined : "paths__default"}>{value || placeholder}</code>
      <button
        type="button"
        className="btn btn--sm"
        onClick={async () => {
          const f = await pickFolder();
          if (f) onChange(f);
        }}
      >
        Choose…
      </button>
      {value && (
        <button type="button" className="link-btn" onClick={() => onChange("")}>
          Use default
        </button>
      )}
    </div>
  );
}

/** Commands allowed without asking, e.g. "git status". */
export function StringList({ value, placeholder, onChange }: { value: string[]; placeholder: string; onChange: (v: string[]) => void }) {
  const [text, setText] = useState("");
  const add = () => {
    const t = text.trim();
    if (t && !value.includes(t)) onChange([...value, t]);
    setText("");
  };
  return (
    <div className="strings">
      <div className="strings__list">
        {value.map((v) => (
          <span key={v} className="strings__item">
            <code>{v}</code>
            <button type="button" aria-label={`Remove ${v}`} onClick={() => onChange(value.filter((x) => x !== v))}>
              ×
            </button>
          </span>
        ))}
      </div>
      <input
        className="text-input"
        value={text}
        placeholder={placeholder}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), add())}
        onBlur={add}
      />
    </div>
  );
}

/** The engine, and the Brave key (kept in the keychain) when it's used. */
export function SearchEnginePicker({ value, onChange }: { value: SearchEngine; onChange: (v: SearchEngine) => void }) {
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [key, setKey] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  useEffect(() => {
    api.hasBraveKey().then(setHasKey).catch(() => setHasKey(false));
  }, []);
  const saveKey = async (k: string) => {
    try {
      await api.setBraveKey(k);
      setHasKey(k.trim() !== "");
      setKey("");
      setMessage(k.trim() ? "Saved in the system keychain." : "Removed.");
    } catch (e) {
      setMessage(String(e));
    }
  };
  return (
    <div className="engine">
      <Segmented
        id="search-engine"
        value={value}
        options={[
          { value: "auto", label: "Automatic" },
          { value: "duckDuckGo", label: "DuckDuckGo" },
          { value: "brave", label: "Brave" },
          { value: "searxng", label: "SearXNG" },
        ]}
        onChange={(v) => onChange(v as SearchEngine)}
      />
      {(value === "auto" || value === "brave") && (
        <div className="engine__key">
          {hasKey ? (
            <>
              <span className="pill pill--ok">Brave key saved</span>
              <button type="button" className="link-btn" onClick={() => saveKey("")}>
                Remove key
              </button>
            </>
          ) : (
            <>
              <input className="text-input" type="password" value={key} placeholder="Brave Search API key" onChange={(e) => setKey(e.target.value)} />
              <button type="button" className="btn btn--sm" disabled={!key.trim()} onClick={() => saveKey(key)}>
                Save
              </button>
            </>
          )}
        </div>
      )}
      {message && <p className="engine__msg">{message}</p>}
    </div>
  );
}
