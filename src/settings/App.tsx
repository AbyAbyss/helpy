import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Ai } from "../bindings/Ai";
import type { HotkeyStatus } from "../bindings/HotkeyStatus";
import type { UsageToday } from "../bindings/UsageToday";
import type { VoiceSupport } from "../bindings/VoiceSupport";
import type { PlatformInfo } from "../bindings/PlatformInfo";
import type { Settings } from "../bindings/Settings";
import { api, asFieldErrors, EVENTS, getValue, type SettingPath, type ValueAt } from "../lib/ipc";
import { useSettings, useTheme } from "../lib/useSettings";
import { BuddyStage, BuddyStylePicker, useCustomBuddy } from "./BuddyPreview";
import { GuidanceStage } from "./GuidancePreview";
import { ApprovalRules, FolderList, FolderPicker, SearchEnginePicker, StringList, ToolToggles } from "./AgentControls";
import { ConnectorList, McpServers, OAuthApps } from "./ConnectorControls";
import { TemplateEditor } from "./TemplateControls";
import { TriggerEditor } from "./TriggerControls";
import type { AgentTools } from "../bindings/AgentTools";
import type { Approvals } from "../bindings/Approvals";
import type { SearchEngine } from "../bindings/SearchEngine";
import { ProvidersEditor } from "./ai/Providers";
import { FallbackEditor, RoutingEditor } from "./ai/Routing";
import { allModels, refKey } from "./ai/routing";
import { ColorField, HotkeyField, MoneyField, NumberField, Segmented, Select, Slider, TextArea, Toggle } from "./controls";
import { FIELDS, SECTIONS, searchFields, type Field, type SectionId } from "./registry";
import { DeepgramField, MicPicker, PiperVoicePicker, ProviderSelect, SampleButton, SystemVoiceSelect, TextInput, WhisperModels } from "./VoiceControls";

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
  const [support, setSupport] = useState<VoiceSupport>({ piper: true });
  const [toast, setToast] = useState<{ text: string; tone: "ok" | "err" } | null>(null);
  const customSrc = useCustomBuddy();
  const search = useRef<HTMLInputElement>(null);
  const content = useRef<HTMLElement>(null);

  // Each section opens at its top.
  useEffect(() => content.current?.scrollTo(0, 0), [section, query === ""]);

  useEffect(() => {
    api.hotkeyStatus().then(setHotkeys);
    api.platform().then(setPlatform);
    api.voiceSupport().then(setSupport);
    const off = listen<HotkeyStatus[]>(EVENTS.hotkeyStatus, (e) => setHotkeys(e.payload));
    return () => void off.then((f) => f());
  }, []);

  // Other windows (the ask panel) can open a specific section.
  useEffect(() => {
    const off = listen<string>(EVENTS.openSection, (e) => {
      if (SECTIONS.some((s) => s.id === e.payload)) {
        setQuery("");
        setSection(e.payload as SectionId);
      }
    });
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

  const commitAi = useCallback(
    async (ai: Ai): Promise<string | null> => {
      try {
        setSettings(await api.setGroup("ai", ai));
        setErrors((prev) => Object.fromEntries(Object.entries(prev).filter(([k]) => !k.startsWith("ai."))));
        return null;
      } catch (e) {
        const list = asFieldErrors(e);
        setErrors((prev) => ({ ...prev, ...Object.fromEntries(list.map((x) => [x.path, x.message])) }));
        return list[0].message;
      }
    },
    [setSettings],
  );

  /** Settings groups shown in a section, e.g. "ai" and "limits" for AI providers. */
  const groupsOf = (id: SectionId) => [...new Set(FIELDS.filter((f) => f.section === id).map((f) => f.path.split(".")[0]))] as (keyof Settings)[];

  const clearSection = (id: SectionId) => {
    const keep = (k: string) => !groupsOf(id).some((g) => k.startsWith(`${g}.`));
    setErrors((prev) => Object.fromEntries(Object.entries(prev).filter(([k]) => keep(k))));
    setDrafts((prev) => Object.fromEntries(Object.entries(prev).filter(([k]) => keep(k))));
  };

  const resetSection = async (id: SectionId) => {
    try {
      const before = settings?.ai.providers.map((p) => p.id) ?? [];
      let next: Settings | null = null;
      for (const g of groupsOf(id)) next = await api.resetSection(g);
      if (next) setSettings(next);
      // Keys of providers the reset removed would otherwise stay in the keychain.
      for (const pid of before.filter((pid) => !next?.ai.providers.some((p) => p.id === pid))) {
        await api.deleteKey(pid).catch(() => {});
      }
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

  const ctx: RowContext = { settings, errors, drafts, hotkeys, customSrc, support, update, commitAi, onError: (m) => setToast({ text: m, tone: "err" }) };
  const extras: Record<string, React.ReactNode> = {
    "Retries and daily budget": <UsageLine settings={settings} />,
    "Your screen": <ScreenRecipients settings={settings} />,
    Voice: <SampleButton />,
  };
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
            {section === "guidance" && <GuidanceStage look={settings.guidance} />}
            {section === "hotkeys" && (
              <p className="page__note">
                Hotkeys marked <span className="pill pill--idle">Not active yet</span> are saved and checked for conflicts now. They start working once their feature is built.
              </p>
            )}
            <Groups fields={FIELDS.filter((f) => f.section === section)} ctx={ctx} extras={extras} />
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
  support: VoiceSupport;
  update: <P extends SettingPath>(path: P, value: ValueAt<P>) => void;
  commitAi: (ai: Ai) => Promise<string | null>;
  onError: (message: string) => void;
};

function Groups({ fields, ctx, extras = {} }: { fields: Field[]; ctx: RowContext; extras?: Record<string, React.ReactNode> }) {
  const visible = fields.filter((f) => !f.when || f.when(ctx.settings));
  const groups = [...new Set(visible.map((f) => f.group))];
  return (
    <>
      {groups.map((g) => (
        <section key={g} className="group" aria-labelledby={`group-${g}`}>
          <h2 id={`group-${g}`} className="group__title">
            {g}
          </h2>
          {extras[g]}
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
  const wide = ["buddyStyle", "providers", "routing", "fallbackChain", "textarea", "whisperModels", "piperVoice", "approvalRules", "toolToggles", "folderList", "stringList", "searchEngine", "connectors", "oauthApps", "mcpServers", "templates", "triggers"].includes(field.control.kind);

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
      // Options this computer can't use are hidden, not shown and then failing.
      return <Segmented id={id} value={value as string} options={c.options.filter((o) => !o.requires || ctx.support[o.requires])} onChange={set} />;
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
    case "textarea":
      return <TextArea id={id} value={saved as string} placeholder={c.placeholder} max={c.max} onCommit={set} />;
    case "money":
      return <MoneyField id={id} value={saved as number | null} emptyLabel={c.emptyLabel} unit={c.unit} invalid={!!ctx.errors[path]} onChange={set} />;
    case "approvalRules":
      return <ApprovalRules value={saved as Approvals} onChange={set} />;
    case "toolToggles":
      return <ToolToggles value={saved as AgentTools} onChange={set} />;
    case "folderList":
      return <FolderList value={saved as string[]} onChange={set} />;
    case "folder":
      return <FolderPicker value={saved as string} placeholder={c.placeholder} onChange={set} />;
    case "stringList":
      return <StringList value={saved as string[]} placeholder={c.placeholder} onChange={set} />;
    case "searchEngine":
      return <SearchEnginePicker value={saved as SearchEngine} onChange={set} />;
    case "text":
      return <TextInput id={id} value={saved as string} placeholder={c.placeholder} invalid={!!ctx.errors[path]} onCommit={set} />;
    case "color":
      return <ColorField id={id} value={value as string} swatches={c.swatches} onChange={set} />;
    case "whisperModels":
      return <WhisperModels value={saved as string} onChange={set} />;
    case "micPicker":
      return <MicPicker value={saved as string | null} denoise={ctx.settings.voiceInput.noiseSuppression} onChange={set} />;
    case "providerSelect":
      return <ProviderSelect id={id} settings={ctx.settings} value={saved as string | null} onChange={set} />;
    case "deepgram":
      return <DeepgramField model={saved as string} invalid={!!ctx.errors[path]} onModel={set} />;
    case "systemVoice":
      return <SystemVoiceSelect id={id} value={saved as string | null} onChange={set} />;
    case "piperVoice":
      return <PiperVoicePicker value={saved as string} onChange={set} />;
    case "providers":
      return <ProvidersEditor ai={ctx.settings.ai} commit={ctx.commitAi} />;
    case "routing":
      return <RoutingEditor ai={ctx.settings.ai} onChange={(routing) => ctx.commitAi({ ...ctx.settings.ai, routing })} />;
    case "fallbackChain":
      return <FallbackEditor ai={ctx.settings.ai} onChange={(fallbackChain) => ctx.commitAi({ ...ctx.settings.ai, fallbackChain })} />;
    case "connectors":
      return <ConnectorList value={ctx.settings.connectors.builtin} onChange={set} />;
    case "oauthApps":
      return <OAuthApps value={ctx.settings.connectors.apps} onChange={set} />;
    case "mcpServers":
      return <McpServers value={ctx.settings.connectors.mcp} onChange={set} />;
    case "templates":
      return <TemplateEditor value={ctx.settings.agents.templates} onChange={set} />;
    case "triggers":
      return <TriggerEditor settings={ctx.settings} value={ctx.settings.agents.triggers} onChange={set} />;
  }
}

/** Today's spending next to the limits, refreshed whenever settings change. */
function UsageLine({ settings }: { settings: Settings }) {
  const [usage, setUsage] = useState<UsageToday | null>(null);
  useEffect(() => void api.usageToday().then(setUsage), [settings]);
  if (!usage) return null;
  const limit = settings.limits.dailyTokenBudget;
  const share = limit > 0 ? Math.min(usage.tokens / limit, 1) : 0;
  return (
    <div className="usage">
      <div className="usage__text">
        <span>Today</span>
        <strong className="mono">{usage.tokens.toLocaleString()}</strong>
        <span>{limit > 0 ? `of ${limit.toLocaleString()} tokens` : "tokens, no limit"}</span>
        {usage.cost > 0 && <span className="mono">· ${usage.cost.toFixed(2)}</span>}
      </div>
      {limit > 0 && (
        <div className={`meter${share > 0.9 ? " meter--hot" : ""}`} role="meter" aria-valuenow={usage.tokens} aria-valuemin={0} aria-valuemax={limit} aria-label="Tokens used today">
          <span style={{ width: `${share * 100}%` }} />
        </div>
      )}
    </div>
  );
}

/** Which provider receives screenshots under the current settings. */
function ScreenRecipients({ settings }: { settings: Settings }) {
  const ai = settings.ai;
  const find = (r: typeof ai.routing.ask) => (r ? allModels(ai).find((m) => refKey(m.ref) === refKey(r)) : undefined);
  const main = find(ai.routing.ask);
  const seer = main?.model.vision ? main : find(ai.routing.visionFallback);
  let text: string;
  if (settings.privacy.capturePaused) text = "Screen capture is paused from the tray, so no screenshots are taken.";
  else if (!main) text = "No model is set up for questions yet.";
  else if (!seer) text = `${main.model.id} can't read images, so screenshots aren't sent anywhere.`;
  else text = `Screenshots go to ${seer.model.id} on ${seer.provider}${isLocalProvider(settings, seer.ref.providerId) ? ", which runs on this computer" : ""}.`;
  return <p className="group__note">{text}</p>;
}

const isLocalProvider = (s: Settings, id: string) => ["ollama", "lmStudio", "llamaCpp"].includes(s.ai.providers.find((p) => p.id === id)?.kind ?? "");

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
    ai: (
      <>
        <rect x="6" y="6" width="12" height="12" rx="2.5" />
        <path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4" />
      </>
    ),
    agents: (
      <>
        <rect x="3" y="4" width="7" height="7" rx="2.2" />
        <rect x="14" y="4" width="7" height="7" rx="2.2" />
        <rect x="3" y="14" width="7" height="7" rx="2.2" />
        <path d="M17.5 14v7M14 17.5h7" />
      </>
    ),
    connectors: (
      <>
        <path d="M9 3v4M15 3v4" />
        <path d="M6.5 7h11v4a5.5 5.5 0 0 1-11 0z" />
        <path d="M12 16.5V21" />
      </>
    ),
    guidance: (
      <>
        <rect x="3.5" y="5" width="11" height="8" rx="2" />
        <path d="M13 15l7 2.6-3 1.1-1.1 3z" />
      </>
    ),
    circle: (
      <>
        <path d="M12 4.5c5 0 8.5 2.6 8.5 6.6 0 4-3.8 6.9-8.8 6.9-5 0-8.2-2.7-8.2-6.4 0-3.3 2.6-5.7 6.5-6.6" />
        <path d="M10.5 11.5h3" />
      </>
    ),
    answerStyle: <path d="M5 5h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-8l-4 3v-3H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2zM7 10h10M7 13h6" />,
    voiceInput: (
      <>
        <rect x="9" y="3" width="6" height="11" rx="3" />
        <path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21" />
      </>
    ),
    voiceOutput: <path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4zM15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11" />,
  };
  return (
    <svg className="nav__icon" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
      {paths[id]}
    </svg>
  );
}
