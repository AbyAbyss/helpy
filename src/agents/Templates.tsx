import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { TemplateInfo } from "../bindings/TemplateInfo";
import type { TemplateParam } from "../bindings/TemplateParam";
import { api } from "../lib/ipc";

/** Templates to start, each with its blanks to fill in. */
export function Templates() {
  const [list, setList] = useState<TemplateInfo[]>([]);
  const [picked, setPicked] = useState<string | null>(null);
  useEffect(() => void api.templates().then(setList, () => {}), []);
  const current = list.find((t) => t.template.id === picked);

  return (
    <article className="detail templates">
      <header className="detail__head">
        <div className="detail__title">
          <h1>Templates</h1>
        </div>
        <div className="detail__actions">
          <button type="button" className="btn btn--quiet" onClick={() => api.openSettingsSection("agents")}>
            Edit templates
          </button>
        </div>
      </header>
      <p className="muted">Tasks to start again and again. Fill in the blanks and Helpy shows the plan first, as always.</p>
      {current ? (
        <TemplateForm key={current.template.id} info={current} onBack={() => setPicked(null)} />
      ) : (
        <div className="tgrid">
          {list.map((t, i) => (
            <button key={t.template.id} type="button" className="tcard" style={{ ["--i" as string]: i }} onClick={() => setPicked(t.template.id)}>
              <span className="tcard__name">{t.template.name}</span>
              <span className="tcard__about">{t.template.description}</span>
              <span className="tcard__meta">
                {t.template.agents.length === 1 ? "1 agent" : `${t.template.agents.length} agents`}
                {!t.builtin && " · yours"}
              </span>
            </button>
          ))}
        </div>
      )}
    </article>
  );
}

function TemplateForm({ info, onBack }: { info: TemplateInfo; onBack: () => void }) {
  const t = info.template;
  const [values, setValues] = useState<Record<string, string>>(() => Object.fromEntries(t.params.map((p) => [p.key, p.default])));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const start = async () => {
    setError(null);
    setBusy(true);
    try {
      await api.templatePlan(t.id, values);
      onBack();
    } catch (e) {
      setError(String(e));
    }
    setBusy(false);
  };

  return (
    <form
      className="box tform"
      onSubmit={(e) => {
        e.preventDefault();
        start();
      }}
    >
      <h2>{t.name}</h2>
      <p className="muted small">{t.description}</p>
      {t.params.map((p, i) => (
        <Blank key={p.key} param={p} value={values[p.key] ?? ""} autoFocus={i === 0} onChange={(v) => setValues((old) => ({ ...old, [p.key]: v }))} />
      ))}
      {t.params.length === 0 && <p className="muted small">Nothing to fill in.</p>}
      {error && <p className="err-text">{error}</p>}
      <div className="box__buttons">
        <button type="submit" className="btn btn--primary" disabled={busy}>
          Plan it
        </button>
        <button type="button" className="btn btn--quiet" onClick={onBack}>
          Back
        </button>
      </div>
    </form>
  );
}

function Blank({ param: p, value, autoFocus, onChange }: { param: TemplateParam; value: string; autoFocus: boolean; onChange: (v: string) => void }) {
  const label = `${p.label}${p.required ? "" : " (optional)"}`;
  if (p.kind === "longText")
    return (
      <label className="approval__field">
        <span>{label}</span>
        <textarea className="field field--area" rows={3} value={value} autoFocus={autoFocus} onChange={(e) => onChange(e.target.value)} />
      </label>
    );
  if (p.kind === "choice")
    return (
      <label className="approval__field">
        <span>{label}</span>
        <select className="field" value={value} onChange={(e) => onChange(e.target.value)}>
          {!p.required && <option value="">No preference</option>}
          {p.options.map((o) => (
            <option key={o}>{o}</option>
          ))}
        </select>
      </label>
    );
  return (
    <label className="approval__field">
      <span>{label}</span>
      <div className="inline">
        <input
          className="field"
          value={value}
          autoFocus={autoFocus}
          inputMode={p.kind === "number" ? "decimal" : undefined}
          onChange={(e) => onChange(e.target.value)}
        />
        {p.kind === "folder" && (
          <button
            type="button"
            className="btn"
            onClick={async () => {
              const dir = await open({ directory: true, multiple: false });
              if (typeof dir === "string") onChange(dir);
            }}
          >
            Choose…
          </button>
        )}
      </div>
    </label>
  );
}
