import { useEffect, useState } from "react";
import type { ParamKind } from "../bindings/ParamKind";
import type { Template } from "../bindings/Template";
import type { TemplateAgent } from "../bindings/TemplateAgent";
import type { TemplateInfo } from "../bindings/TemplateInfo";
import type { TemplateParam } from "../bindings/TemplateParam";
import type { ToolGroup } from "../bindings/ToolGroup";
import { api } from "../lib/ipc";
import { Toggle } from "./controls";

const KINDS: { value: ParamKind; label: string }[] = [
  { value: "text", label: "Text" },
  { value: "longText", label: "Long text" },
  { value: "folder", label: "Folder" },
  { value: "number", label: "Number" },
  { value: "choice", label: "Choice" },
];

const newId = () => `t${Date.now()}`;

/** The user's templates, editable; built-in ones can be copied to edit. */
export function TemplateEditor({ value, onChange }: { value: Template[]; onChange: (v: Template[]) => void }) {
  const [builtins, setBuiltins] = useState<TemplateInfo[]>([]);
  const [groups, setGroups] = useState<ToolGroup[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  useEffect(() => {
    api.templates().then((l) => setBuiltins(l.filter((t) => t.builtin)), () => {});
    api.toolGroups().then(setGroups, () => {});
  }, []);

  const add = (t: Template) => {
    onChange([...value, t]);
    setOpen(t.id);
  };
  const blank = (): Template => ({
    id: newId(),
    name: "My template",
    description: "",
    params: [{ key: "topic", label: "Topic", kind: "text", default: "", options: [], required: true }],
    agents: [{ name: "Agent", goal: "Do something about {topic}.", tools: ["search", "web"], keepOpen: false, after: [] }],
    folders: [],
  });

  return (
    <div className="providers">
      {value.length === 0 && (
        <div className="providers__empty">
          <p>
            <strong>No templates of your own yet.</strong> Make one here, copy a built-in one, or use "Save as template" on any agent
            in the agent panel.
          </p>
        </div>
      )}
      {value.map((t) => (
        <div key={t.id} className={`pcard${open === t.id ? " is-open" : ""}`}>
          <div className="pcard__head">
            <span className="pmark" aria-hidden="true">
              {t.name.slice(0, 2)}
            </span>
            <div className="pcard__title">
              <strong>{t.name}</strong>
              <span className="muted">{t.description || `${t.agents.length} agent${t.agents.length === 1 ? "" : "s"}`}</span>
            </div>
            <button type="button" className="btn btn--ghost btn--sm" onClick={() => setOpen(open === t.id ? null : t.id)}>
              {open === t.id ? "Done" : "Edit"}
            </button>
          </div>
          {open === t.id && (
            <TemplateForm
              template={t}
              groups={groups}
              onChange={(n) => onChange(value.map((x) => (x.id === t.id ? n : x)))}
              onRemove={() => onChange(value.filter((x) => x.id !== t.id))}
            />
          )}
        </div>
      ))}
      <div className="pform__foot">
        <button type="button" className="btn btn--ghost add-btn" onClick={() => add(blank())}>
          + New template
        </button>
        {builtins.length > 0 && (
          <select
            className="tpl-copy"
            value=""
            onChange={(e) => {
              const b = builtins.find((x) => x.template.id === e.target.value);
              if (b) add({ ...structuredClone(b.template), id: newId(), name: `${b.template.name} (mine)` });
            }}
          >
            <option value="">Copy a built-in one…</option>
            {builtins.map((b) => (
              <option key={b.template.id} value={b.template.id}>
                {b.template.name}
              </option>
            ))}
          </select>
        )}
      </div>
    </div>
  );
}

/** Edits a draft; saves when a field loses focus. */
function TemplateForm(props: { template: Template; groups: ToolGroup[]; onChange: (t: Template) => void; onRemove: () => void }) {
  const [t, setT] = useState(props.template);
  useEffect(() => setT(props.template), [props.template]);
  const save = (draft: Template = t) => {
    const next = { ...draft, folders: draft.folders.map((f) => f.trim()).filter(Boolean) };
    setT(next);
    if (JSON.stringify(next) !== JSON.stringify(props.template)) props.onChange(next);
  };
  const setParam = (i: number, p: Partial<TemplateParam>) => setT({ ...t, params: t.params.map((x, j) => (j === i ? { ...x, ...p } : x)) });
  const setAgent = (i: number, a: Partial<TemplateAgent>) => setT({ ...t, agents: t.agents.map((x, j) => (j === i ? { ...x, ...a } : x)) });

  return (
    <div className="pform" onBlur={() => save()}>
      <div className="pform__grid">
        <label className="field">
          <span>Name</span>
          <input value={t.name} onChange={(e) => setT({ ...t, name: e.target.value })} />
        </label>
        <label className="field">
          <span>What it's for</span>
          <input value={t.description} onChange={(e) => setT({ ...t, description: e.target.value })} />
        </label>
      </div>

      <div className="kv">
        <div className="rules__label">Blanks</div>
        <p className="rules__help">Use a blank in a goal as {"{key}"}, e.g. "Find {"{count}"} desks".</p>
        {t.params.map((p, i) => (
          <div key={i} className="tpl-param">
            <input className="mono" value={p.key} placeholder="key" onChange={(e) => setParam(i, { key: e.target.value.replace(/[^A-Za-z0-9_]/g, "") })} />
            <input value={p.label} placeholder="Label" onChange={(e) => setParam(i, { label: e.target.value })} />
            <select value={p.kind} onChange={(e) => save({ ...t, params: t.params.map((x, j) => (j === i ? { ...x, kind: e.target.value as ParamKind } : x)) })}>
              {KINDS.map((k) => (
                <option key={k.value} value={k.value}>
                  {k.label}
                </option>
              ))}
            </select>
            <input
              value={p.kind === "choice" ? p.options.join(", ") : p.default}
              placeholder={p.kind === "choice" ? "Options, comma separated" : "Default"}
              onChange={(e) =>
                p.kind === "choice"
                  ? setParam(i, { options: e.target.value.split(",").map((o) => o.trim()).filter(Boolean) })
                  : setParam(i, { default: e.target.value })
              }
            />
            <label className="kv__secret">
              <input type="checkbox" checked={p.required} onChange={(e) => save({ ...t, params: t.params.map((x, j) => (j === i ? { ...x, required: e.target.checked } : x)) })} />
              Needed
            </label>
            <button type="button" className="link-btn" onClick={() => save({ ...t, params: t.params.filter((_, j) => j !== i) })}>
              Remove
            </button>
          </div>
        ))}
        <button
          type="button"
          className="link-btn"
          onClick={() => setT({ ...t, params: [...t.params, { key: `blank${t.params.length + 1}`, label: "", kind: "text", default: "", options: [], required: false }] })}
        >
          + Add a blank
        </button>
      </div>

      {t.agents.map((a, i) => (
        <div key={i} className="tpl-agent">
          <div className="pform__grid">
            <label className="field">
              <span>Agent {t.agents.length > 1 ? i + 1 : ""} name</span>
              <input value={a.name} onChange={(e) => setAgent(i, { name: e.target.value })} />
            </label>
            <div className="crow">
              <span className="rules__help">Stays open for changes</span>
              <Toggle id={`tpl-open-${i}`} checked={a.keepOpen} onChange={(keepOpen) => save({ ...t, agents: t.agents.map((x, j) => (j === i ? { ...x, keepOpen } : x)) })} />
            </div>
          </div>
          <label className="field">
            <span>Goal</span>
            <div className="textarea">
              <textarea rows={3} value={a.goal} onChange={(e) => setAgent(i, { goal: e.target.value })} />
            </div>
          </label>
          <div className="chips">
            {props.groups.map((g) => {
              const on = a.tools.includes(g.id);
              return (
                <button
                  key={g.id}
                  type="button"
                  className="chip"
                  aria-checked={on}
                  role="checkbox"
                  onClick={() => save({ ...t, agents: t.agents.map((x, j) => (j === i ? { ...x, tools: on ? x.tools.filter((y) => y !== g.id) : [...x.tools, g.id] } : x)) })}
                >
                  {g.label}
                </button>
              );
            })}
          </div>
          {i > 0 && (
            <div className="chips">
              <span className="rules__help">Starts after</span>
              {t.agents.slice(0, i).map((o) => {
                const on = a.after.includes(o.name);
                return (
                  <button
                    key={o.name}
                    type="button"
                    className="chip"
                    aria-checked={on}
                    role="checkbox"
                    onClick={() => save({ ...t, agents: t.agents.map((x, j) => (j === i ? { ...x, after: on ? x.after.filter((y) => y !== o.name) : [...x.after, o.name] } : x)) })}
                  >
                    {o.name}
                  </button>
                );
              })}
            </div>
          )}
          {t.agents.length > 1 && (
            <button type="button" className="link-btn" onClick={() => save({ ...t, agents: t.agents.filter((_, j) => j !== i).map((x) => ({ ...x, after: x.after.filter((n) => n !== a.name) })) })}>
              Remove this agent
            </button>
          )}
        </div>
      ))}
      {t.agents.length < 5 && (
        <button
          type="button"
          className="link-btn"
          onClick={() => save({ ...t, agents: [...t.agents, { name: `Agent ${t.agents.length + 1}`, goal: "", tools: [], keepOpen: false, after: [] }] })}
        >
          + Add an agent
        </button>
      )}

      <label className="field">
        <span>Folders it works in (one per line; approved when it starts)</span>
        <div className="textarea">
          <textarea rows={2} className="mono" value={t.folders.join("\n")} onChange={(e) => setT({ ...t, folders: e.target.value.split("\n") })} />
        </div>
      </label>

      <div className="pform__foot">
        <button type="button" className="btn btn--ghost" onClick={props.onRemove}>
          Delete template
        </button>
      </div>
    </div>
  );
}
