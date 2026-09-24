import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { PlatformInfo } from "../bindings/PlatformInfo";
import type { ThemePreference } from "../bindings/ThemePreference";
import { api, asCommandError, EVENTS, listen, pickFile } from "../lib/ipc";
import { useSettings } from "../lib/useSettings";
import { Buddy } from "../buddy/Buddy";
import { SettingsCtx, useCtx, type Ctx } from "./context";
import { Icon } from "./icons";
import { NAV_GROUPS, SECTIONS, SETTINGS, searchSettings, sectionOf, type SectionKey, type SettingDef } from "./registry";
import { Button } from "./controls/controls";
import { BuddyPreview } from "./sections/BuddyPreview";
import "./settings.css";

function applyTheme(t: ThemePreference | undefined) {
  const root = document.documentElement;
  if (!t || t === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", t);
}

function initialSection(): SectionKey {
  const h = location.hash.slice(1) as SectionKey;
  return SECTIONS.some((s) => s.key === h) ? h : "general";
}

interface Toast {
  id: number;
  text: string;
  tone: "ok" | "error";
}

export function SettingsApp() {
  const store = useSettings();
  const [platform, setPlatform] = useState<PlatformInfo | null>(null);
  const [hotkeyErrors, setHotkeyErrors] = useState<Record<string, string>>({});
  const [section, setSection] = useState<SectionKey>(initialSection);
  const [query, setQuery] = useState("");
  const [toasts, setToasts] = useState<Toast[]>([]);
  const searchRef = useRef<HTMLInputElement>(null);
  const mainRef = useRef<HTMLElement>(null);

  useEffect(() => {
    api.platform().then(setPlatform);
    api.hotkeyStatus().then(setHotkeyErrors);
    const subs = [
      listen<Record<string, string>>(EVENTS.hotkeyStatus, setHotkeyErrors),
      listen<string>(EVENTS.settingsNavigate, (s) => go(s as SectionKey)),
    ];
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      subs.forEach((s) => s.then((f) => f()));
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  useEffect(() => applyTheme(store.settings?.general.theme), [store.settings?.general.theme]);

  const toast = useCallback((text: string, tone: "ok" | "error" = "ok") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, text, tone }]);
    window.setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), tone === "error" ? 6000 : 2800);
  }, []);

  useEffect(() => {
    if (store.lastError) toast(store.lastError, "error");
  }, [store.lastError, toast]);

  const go = (key: SectionKey, focusId?: string) => {
    setQuery("");
    setSection(key);
    history.replaceState(null, "", `#${key}`);
    requestAnimationFrame(() => {
      if (focusId) {
        const el = document.getElementById(`row-${focusId}`);
        el?.scrollIntoView({ block: "center", behavior: "smooth" });
        el?.classList.add("row-flash");
        window.setTimeout(() => el?.classList.remove("row-flash"), 1400);
      } else mainRef.current?.scrollTo({ top: 0 });
    });
  };

  const ctx: Ctx | null = useMemo(
    () => (store.settings && platform ? { store, platform, hotkeyErrors, toast } : null),
    [store, platform, hotkeyErrors, toast],
  );
  if (!ctx) return <div className="boot" />;

  const results = query.trim() ? searchSettings(query) : null;

  return (
    <SettingsCtx.Provider value={ctx}>
      <div className="app">
        <Sidebar current={section} onGo={go} query={query} setQuery={setQuery} searchRef={searchRef} />
        <main className="main" ref={mainRef}>
          {results ? <SearchResults query={query} results={results} onGo={go} /> : <SectionPage key={section} sectionKey={section} />}
        </main>
      </div>
      <div className="toasts" role="status" aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className="toast" data-tone={t.tone}>
            <Icon name={t.tone === "ok" ? "check" : "alert"} />
            {t.text}
          </div>
        ))}
      </div>
    </SettingsCtx.Provider>
  );
}

/* ------------------------------------------------------------------ Sidebar */

function Sidebar({
  current,
  onGo,
  query,
  setQuery,
  searchRef,
}: {
  current: SectionKey;
  onGo: (k: SectionKey) => void;
  query: string;
  setQuery: (q: string) => void;
  searchRef: React.RefObject<HTMLInputElement | null>;
}) {
  const { platform, store, toast } = useCtx();
  const mod = platform.os === "macos" ? "⌘" : "Ctrl";

  const doExport = async () => {
    const path = await pickFile({ title: "Export Helpy settings", name: "Helpy settings", extensions: ["json"], save: true, defaultPath: "helpy-settings.json" });
    if (!path) return;
    try {
      await api.exportSettings(path);
      toast("Settings exported. API keys and tokens are never included.");
    } catch (e) {
      const err = asCommandError(e);
      toast(err.kind === "message" ? err.message : "Export failed", "error");
    }
  };
  const doImport = async () => {
    const path = await pickFile({ title: "Import Helpy settings", name: "Helpy settings", extensions: ["json"] });
    if (!path) return;
    try {
      store.replace(await api.importSettings(path));
      toast("Settings imported");
    } catch (e) {
      const err = asCommandError(e);
      toast(err.kind === "invalid" ? `Not imported: ${err.errors.length} value${err.errors.length === 1 ? " is" : "s are"} out of range (${err.errors[0].path}).` : err.message, "error");
    }
  };

  return (
    <nav className="side" aria-label="Settings sections">
      <div className="brand">
        <Buddy style="pip" size={26} activity="idle" animate={false} />
        <span className="brand-name">Helpy</span>
        <span className="brand-ver">{platform.version}</span>
      </div>

      <label className="search">
        <Icon name="search" />
        <input
          ref={searchRef}
          type="search"
          placeholder="Search settings"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && setQuery("")}
          aria-label="Search settings"
        />
        {!query && <kbd>{mod} K</kbd>}
      </label>

      <div className="nav">
        {NAV_GROUPS.map((g) => (
          <div key={g} className="nav-group">
            <h2>{g}</h2>
            {SECTIONS.filter((s) => s.nav === g).map((s) => (
              <button
                key={s.key}
                type="button"
                className="nav-item"
                aria-current={!query && current === s.key ? "page" : undefined}
                data-later={s.phase ? "yes" : "no"}
                onClick={() => onGo(s.key)}
              >
                <Icon name={s.icon} />
                <span>{s.title}</span>
                {s.phase && <small>Soon</small>}
              </button>
            ))}
          </div>
        ))}
      </div>

      <div className="side-foot">
        <button type="button" className="foot-btn" onClick={doImport}>
          <Icon name="download" />
          Import
        </button>
        <button type="button" className="foot-btn" onClick={doExport}>
          <Icon name="upload" />
          Export
        </button>
      </div>
    </nav>
  );
}

/* ------------------------------------------------------------------ Section page */

function SectionPage({ sectionKey }: { sectionKey: SectionKey }) {
  const { store, platform, toast } = useCtx();
  const sec = sectionOf(sectionKey);
  const defs = SETTINGS.filter((s) => s.section === sectionKey);
  const groups = [...new Set(defs.map((d) => d.group))];
  const [confirmReset, setConfirmReset] = useState(false);

  const reset = async () => {
    if (!sec.resetId) return;
    store.replace(await api.resetSection(sec.resetId));
    setConfirmReset(false);
    toast(`${sec.title} reset to defaults`);
  };

  const waylandNote =
    platform.wayland && (sectionKey === "buddy" || sectionKey === "hotkeys")
      ? sectionKey === "buddy"
        ? "You're on Wayland. It doesn't let apps read the global pointer position, so the buddy can't follow the cursor here. Use an X11 session for the full experience."
        : "You're on Wayland. Global hotkeys depend on your desktop and may not fire. Helpy will use the desktop portal once it's available."
      : null;

  return (
    <article className="page">
      <header className="page-head">
        <div>
          <h1>{sec.title}</h1>
          <p>{sec.blurb}</p>
        </div>
        {sec.resetId &&
          (confirmReset ? (
            <div className="confirm-inline">
              <span>Reset {sec.title.toLowerCase()}?</span>
              <Button variant="ghost" onClick={() => setConfirmReset(false)}>
                Cancel
              </Button>
              <Button variant="primary" onClick={reset}>
                Reset
              </Button>
            </div>
          ) : (
            <Button variant="ghost" icon={<Icon name="reset" />} onClick={() => setConfirmReset(true)}>
              Reset to defaults
            </Button>
          ))}
      </header>

      {sec.phase && (
        <div className="notice" data-tone="info">
          <strong>Arrives in Phase {sec.phase}.</strong> Here's everything this section will control, so you know where to find it.
        </div>
      )}
      {waylandNote && (
        <div className="notice" data-tone="warn">
          <Icon name="alert" />
          <span>{waylandNote}</span>
        </div>
      )}

      {sectionKey === "buddy" && <BuddyPreview />}

      {groups.map((g) => (
        <section key={g} className="group">
          <h2>{g}</h2>
          <div className="panel">
            {defs
              .filter((d) => d.group === g)
              .filter((d) => !d.when || d.when(store.get))
              .map((d) => (
                <Row key={d.id} def={d} />
              ))}
          </div>
        </section>
      ))}
    </article>
  );
}

function Row({ def, onOpen }: { def: SettingDef; onOpen?: () => void }) {
  const { store } = useCtx();
  const error = store.errors[def.id];
  const custom = def.id.startsWith("hotkeys.") || def.id === "buddy.style";
  const control: ReactNode = def.control?.();
  return (
    <div className="row" id={`row-${def.id}`} data-wide={def.wide ? "yes" : "no"} data-later={def.phase ? "yes" : "no"}>
      <div className="row-text">
        <div className="row-label">
          {onOpen && (
            <button type="button" className="row-crumb" onClick={onOpen}>
              {sectionOf(def.section).title}
            </button>
          )}
          {def.label}
          {def.phase && <span className="tag">Phase {def.phase}</span>}
        </div>
        {def.help && <p className="row-help">{def.help}</p>}
      </div>
      {control && (
        <div className="row-control">
          {control}
          {error && !custom && <p className="row-error">{error}</p>}
        </div>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ Search */

function SearchResults({ query, results, onGo }: { query: string; results: SettingDef[]; onGo: (k: SectionKey, id?: string) => void }) {
  const live = results.filter((r) => !r.phase);
  const later = results.filter((r) => r.phase);
  return (
    <article className="page">
      <header className="page-head">
        <div>
          <h1>Results for “{query.trim()}”</h1>
          <p>
            {results.length === 0
              ? "Nothing matches. Try a shorter word, like “voice” or “hide”."
              : `${results.length} setting${results.length === 1 ? "" : "s"}. Change them right here, or open their section.`}
          </p>
        </div>
      </header>
      {live.length > 0 && (
        <section className="group">
          <h2>Available now</h2>
          <div className="panel">
            {live.map((d) => (
              <Row key={d.id} def={d} onOpen={() => onGo(d.section, d.id)} />
            ))}
          </div>
        </section>
      )}
      {later.length > 0 && (
        <section className="group">
          <h2>Coming later</h2>
          <div className="panel">
            {later.map((d) => (
              <Row key={d.id} def={d} onOpen={() => onGo(d.section, d.id)} />
            ))}
          </div>
        </section>
      )}
    </article>
  );
}
