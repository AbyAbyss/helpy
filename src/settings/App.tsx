import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { HotkeyStatus } from "../bindings/HotkeyStatus";
import type { PlatformInfo } from "../bindings/PlatformInfo";
import type { Settings } from "../bindings/Settings";
import { api, asFieldErrors, EVENTS, getValue, type SettingPath, type ValueAt } from "../lib/ipc";
import { useSettings, useTheme } from "../lib/useSettings";
import { BuddyStage, BuddyStylePicker, useCustomBuddy } from "./BuddyPreview";
import { HotkeyField, NumberField, Segmented, Select, Slider, Toggle } from "./controls";
import { FIELDS, SECTIONS, searchFields, type Field, type SectionId } from "./registry";

type Errors = Record<string, string>;

export function App() {
  const [settings, setSettings] = useSettings();
  useTheme(settings);
  const [section, setSection] = useState<SectionId>("general");
  const [query, setQuery] = useState("");
  const [errors, setErrors] = useState<Errors>({});
  const [drafts, setDrafts] = useState<Record<string, unknown>>({});
  const [hotkeys, setHotkeys] = useState<HotkeyStatus[]>([]);
  const [platform, setPlatform] = useState<PlatformInfo | null>(null);
  const [toast, setToast] = useState<{ text: string; tone: "ok" | "err" } | null>(null);
  const customSrc = useCustomBuddy();
  const search = useRef<HTMLInputElement>(null);
  const content = useRef<HTMLElement>(null);

  // Each section opens at its top.
  useEffect(() => content.current?.scrollTo(0, 0), [section, query === ""]);

  useEffect(() => {
    api.hotkeyStatus().then(setHotkeys);
    api.platform().then(setPlatform);
    const off = listen<HotkeyStatus[]>(EVENTS.hotkeyStatus, (e) => setHotkeys(e.payload));
    return () => void off.then((f) => f());
  }, []);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 4000);
    return () => clearTimeout(t);
  }, [toast]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        search.current?.focus();
        search.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const update = useCallback(
    async <P extends SettingPath>(path: P, value: ValueAt<P>) => {
      try {
        setSettings(await api.set(path, value));
        setErrors(({ [path]: _, ...rest }) => rest);
        setDrafts(({ [path]: _, ...rest }) => rest);
      } catch (e) {
        const list = asFieldErrors(e);
        const mine = list.find((x) => x.path === path) ?? list[0];
        setErrors((prev) => ({ ...prev, [path]: mine.message }));
        setDrafts((prev) => ({ ...prev, [path]: value }));
      }
    },
    [setSettings],
  );

  const clearSection = (id: SectionId) => {
    const keep = (k: string) => !k.startsWith(`${id}.`);
    setErrors((prev) => Object.fromEntries(Object.entries(prev).filter(([k]) => keep(k))));
    setDrafts((prev) => Object.fromEntries(Object.entries(prev).filter(([k]) => keep(k))));
  };

  const resetSection = async (id: SectionId) => {
    try {
      setSettings(await api.resetSection(id));
      clearSection(id);
      setToast({ text: `${SECTIONS.find((s) => s.id === id)!.title} is back to its defaults`, tone: "ok" });
    } catch (e) {
      setToast({ text: asFieldErrors(e)[0].message, tone: "err" });
    }
  };

  const exportSettings = async () => {
    const path = await save({ defaultPath: "helpy-settings.json", filters: [{ name: "Helpy settings", extensions: ["json"] }] });
    if (!path) return;
    try {
      await api.exportTo(path);
      setToast({ text: "Settings exported. API keys and sign-ins are never included", tone: "ok" });
    } catch (e) {
      setToast({ text: String(e), tone: "err" });
    }
  };

  const importSettings = async () => {
    const path = await open({ multiple: false, filters: [{ name: "Helpy settings", extensions: ["json"] }] });
    if (typeof path !== "string") return;
    try {
      setSettings(await api.importFrom(path));
      setErrors({});
      setDrafts({});
      setToast({ text: "Settings imported", tone: "ok" });
    } catch (e) {
      const list = asFieldErrors(e);
      const detail = list.map((x) => (x.path ? `${x.path}: ${x.message}` : x.message)).join("; ");
      setToast({ text: `Nothing was changed. ${detail}`, tone: "err" });
    }
  };

  if (!settings) return <div className="app app--loading" />;

  const ctx: RowContext = { settings, errors, drafts, hotkeys, customSrc, update, onError: (m) => setToast({ text: m, tone: "err" }) };
  const results = searchFields(query);
  const current = SECTIONS.find((s) => s.id === section)!;

  return (
    <div className="app">
      <nav className="rail" aria-label="Settings sections">
        <div className="brand">
          <BrandMark />
          <span className="brand__name">Helpy</span>
        </div>
        <label className="search">
          <SearchIcon />
          <input
            ref={search}
            id="settings-search"
            type="search"
            placeholder="Search settings"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Escape" && setQuery("")}
            aria-label="Search settings"
          />
          <kbd className="search__kbd">{navigator.platform.includes("Mac") ? "⌘F" : "Ctrl F"}</kbd>
        </label>
        <ul className="nav">
          {SECTIONS.map((s) => (
            <li key={s.id}>
              <button
                type="button"
                className="nav__item"
                aria-current={!query && section === s.id ? "page" : undefined}
                onClick={() => {
                  setQuery("");
                  setSection(s.id);
                }}
              >
                <SectionIcon id={s.id} />
                {s.title}
                {FIELDS.some((f) => f.section === s.id && errors[f.path]) && <span className="nav__dot" aria-label="Has a problem" />}
              </button>
            </li>
          ))}
        </ul>
        <div className="rail__foot">
          <button type="button" className="link-btn" onClick={importSettings}>
            Import…
          </button>
          <button type="button" className="link-btn" onClick={exportSettings}>
            Export…
          </button>
        </div>
      </nav>

      <main ref={content} className="content">
        {query ? (
          <SearchResults query={query} results={results} ctx={ctx} onOpen={(id) => { setQuery(""); setSection(id); }} />
        ) : (
          <div className="page" key={section}>
            <header className="page__head">
              <div>
                <h1>{current.title}</h1>
                <p className="page__blurb">{current.blurb}</p>
              </div>
              <ResetButton title={current.title} onReset={() => resetSection(section)} />
            </header>

            {section === "general" && platform && platform.limitations.length > 0 && (
              <aside className="notice" role="note">
                <strong>On this system</strong>
                <ul>
                  {platform.limitations.map((l) => (
                    <li key={l}>{l}</li>
                  ))}
                </ul>
              </aside>
            )}
            {section === "buddy" && <BuddyStage buddy={settings.buddy} customSrc={customSrc} />}
            {section === "hotkeys" && (
              <p className="page__note">
                Hotkeys marked <span className="pill pill--idle">Not active yet</span> are saved and checked for conflicts now. They start working once their feature is built.
              </p>
            )}
            <Groups fields={FIELDS.filter((f) => f.section === section)} ctx={ctx} />
          </div>
        )}
      </main>

      <div className="toast-region" aria-live="polite">
        {toast && <div className={`toast toast--${toast.tone}`}>{toast.text}</div>}
      </div>
    </div>
  );
}

type RowContext = {
  settings: Settings;
  errors: Errors;
  drafts: Record<string, unknown>;
  hotkeys: HotkeyStatus[];
  customSrc: string | null;
  update: <P extends SettingPath>(path: P, value: ValueAt<P>) => void;
  onError: (message: string) => void;
};

function Groups({ fields, ctx }: { fields: Field[]; ctx: RowContext }) {
  const visible = fields.filter((f) => !f.when || f.when(ctx.settings));
  const groups = [...new Set(visible.map((f) => f.group))];
  return (
    <>
      {groups.map((g) => (
        <section key={g} className="group" aria-labelledby={`group-${g}`}>
          <h2 id={`group-${g}`} className="group__title">
            {g}
          </h2>
          <div className="panel">
            {visible
              .filter((f) => f.group === g)
              .map((f) => (
                <Row key={f.path} field={f} ctx={ctx} />
              ))}
          </div>
        </section>
      ))}
    </>
  );
}

function SearchResults({ query, results, ctx, onOpen }: { query: string; results: Field[]; ctx: RowContext; onOpen: (id: SectionId) => void }) {
  return (
    <div className="page">
      <header className="page__head">
        <div>
          <h1>Search</h1>
          <p className="page__blurb">
            {results.length === 0 ? `No settings match “${query}”.` : `${results.length} ${results.length === 1 ? "setting matches" : "settings match"} “${query}”.`}
          </p>
        </div>
      </header>
      {SECTIONS.filter((s) => results.some((r) => r.section === s.id)).map((s) => (
        <section key={s.id} className="group">
          <h2 className="group__title">
            <button type="button" className="link-btn" onClick={() => onOpen(s.id)}>
              {s.title} →
            </button>
          </h2>
          <div className="panel">
            {results
              .filter((r) => r.section === s.id)
              .map((f) => (
                <Row key={f.path} field={f} ctx={ctx} />
              ))}
          </div>
        </section>
      ))}
    </div>
  );
}

function Row({ field, ctx }: { field: Field; ctx: RowContext }) {
  const id = `setting-${field.path.replace(".", "-")}`;
  const error = ctx.errors[field.path];
  const status = field.control.kind === "hotkey" ? ctx.hotkeys.find((h) => h.action === field.path.split(".")[1]) : undefined;
  const warning = !error ? status?.warning ?? (status?.state === "failed" ? status.error : null) : null;
  const wide = field.control.kind === "buddyStyle";

  return (
    <div className={`row${wide ? " row--wide" : ""}${error ? " has-error" : ""}`}>
      <div className="row__text">
        <label className="row__label" htmlFor={id}>
          {field.label}
          {status && <HotkeyPill status={status} />}
        </label>
        {field.help && <p className="row__help">{field.help}</p>}
        {error && (
          <p className="row__msg row__msg--err" role="alert">
            {error}
          </p>
        )}
        {warning && <p className={`row__msg ${status?.state === "failed" ? "row__msg--err" : "row__msg--warn"}`}>{warning}</p>}
      </div>
      <div className="row__control">
        <ControlFor id={id} field={field} ctx={ctx} />
      </div>
    </div>
  );
}

function HotkeyPill({ status }: { status: HotkeyStatus }) {
  switch (status.state) {
    case "active":
      return <span className="pill pill--ok">Active</span>;
    case "notYetAvailable":
      return <span className="pill pill--idle">Not active yet</span>;
    case "failed":
      return <span className="pill pill--err">Not working</span>;
    default:
      return null;
  }
}

function ControlFor({ id, field, ctx }: { id: string; field: Field; ctx: RowContext }) {
  const path = field.path;
  const saved = getValue(ctx.settings, path);
  const value = path in ctx.drafts ? ctx.drafts[path] : saved;
  // The registry pairs each path with a control of the right value type.
  const set = (v: unknown) => ctx.update(path, v as never);
  const c = field.control;
  switch (c.kind) {
    case "toggle":
      return <Toggle id={id} checked={value as boolean} onChange={set} />;
    case "segmented":
      return <Segmented id={id} value={value as string} options={c.options} onChange={set} />;
    case "select":
      return <Select id={id} value={value as string} options={c.options} onChange={set} />;
    case "slider":
      return <Slider id={id} value={value as number} min={c.min} max={c.max} step={c.step} format={c.format} ends={c.ends} onChange={set} />;
    case "number":
      return <NumberField id={id} value={saved as number} min={c.min} max={c.max} unit={c.unit} invalid={!!ctx.errors[path]} onChange={set} />;
    case "hotkey":
      return (
        <HotkeyField
          id={id}
          value={value as string}
          invalid={!!ctx.errors[path]}
          onRecordingChange={(on) => void (on ? api.suspendHotkeys() : api.resumeHotkeys())}
          onChange={set}
        />
      );
    case "buddyStyle":
      return <BuddyStylePicker value={ctx.settings.buddy.style} customSrc={ctx.customSrc} onChange={set} onError={ctx.onError} />;
  }
}

function ResetButton({ title, onReset }: { title: string; onReset: () => void }) {
  const [confirming, setConfirming] = useState(false);
  if (!confirming)
    return (
      <button type="button" className="btn btn--ghost" onClick={() => setConfirming(true)}>
        Reset to defaults
      </button>
    );
  return (
    <div className="confirm" role="group" aria-label={`Reset ${title}`}>
      <span>Reset {title}?</span>
      <button type="button" className="btn btn--danger" onClick={() => { setConfirming(false); onReset(); }} autoFocus>
        Reset
      </button>
      <button type="button" className="btn btn--ghost" onClick={() => setConfirming(false)}>
        Cancel
      </button>
    </div>
  );
}

function BrandMark() {
  return (
    <svg viewBox="0 0 64 64" width="26" height="26" aria-hidden="true">
      <path d="M6 6 L26 13.5 A22 22 0 1 1 13.5 26 Z" fill="var(--accent)" />
      <ellipse cx="29" cy="33" rx="3.2" ry="4.4" fill="var(--rail)" />
      <ellipse cx="41" cy="33" rx="3.2" ry="4.4" fill="var(--rail)" />
    </svg>
  );
}

function SearchIcon() {
  return (
    <svg className="search__icon" viewBox="0 0 20 20" width="16" height="16" aria-hidden="true">
      <circle cx="9" cy="9" r="5.5" />
      <path d="m13.2 13.2 3.8 3.8" />
    </svg>
  );
}

function SectionIcon({ id }: { id: SectionId }) {
  const paths: Record<SectionId, React.ReactNode> = {
    general: (
      <>
        <path d="M4 6h8M16 6h4M4 12h3M11 12h9M4 18h11M19 18h1" />
        <circle cx="14" cy="6" r="2" />
        <circle cx="9" cy="12" r="2" />
        <circle cx="17" cy="18" r="2" />
      </>
    ),
    buddy: <path d="M4 4l6 2.3a8 8 0 1 1-3.7 3.7z" />,
    hotkeys: (
      <>
        <rect x="3" y="6" width="18" height="12" rx="2.5" />
        <path d="M7 10h.01M11 10h.01M15 10h.01M8 14h8" />
      </>
    ),
  };
  return (
    <svg className="nav__icon" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
      {paths[id]}
    </svg>
  );
}
