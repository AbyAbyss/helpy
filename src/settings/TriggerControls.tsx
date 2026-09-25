import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Settings } from "../bindings/Settings";
import type { TemplateInfo } from "../bindings/TemplateInfo";
import type { ToolGroup } from "../bindings/ToolGroup";
import type { Trigger } from "../bindings/Trigger";
import type { TriggerStatus } from "../bindings/TriggerStatus";
import { api } from "../lib/ipc";
import { FolderPicker } from "./AgentControls";
import { Segmented, Toggle } from "./controls";

// ---------- Schedules in plain words ----------

export type Repeat =
  | { kind: "daily"; time: string }
  | { kind: "weekdays"; time: string }
  | { kind: "weekly"; days: number[]; time: string }
  | { kind: "hours"; every: number }
  | { kind: "minutes"; every: number }
  | { kind: "custom"; cron: string };

const DAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

const pad = (n: number) => String(n).padStart(2, "0");

/** "0 9 * * 1-5" → weekdays at 09:00; anything else is custom. */
export function fromCron(cron: string): Repeat {
  const f = cron.trim().split(/\s+/);
  if (f.length !== 5) return { kind: "custom", cron };
  const [m, h, dom, mon, dow] = f;
  const num = (s: string) => /^\d+$/.test(s);
  if (dom === "*" && mon === "*") {
    if (num(m) && num(h)) {
      const time = `${pad(+h)}:${pad(+m)}`;
      if (dow === "*") return { kind: "daily", time };
      if (dow === "1-5") return { kind: "weekdays", time };
      if (/^[0-7](,[0-7])*$/.test(dow)) return { kind: "weekly", time, days: [...new Set(dow.split(",").map((d) => +d % 7))].sort() };
    }
    if (m === "0" && dow === "*" && /^\*\/\d+$/.test(h)) return { kind: "hours", every: +h.slice(2) };
    if (h === "*" && dow === "*" && /^\*\/\d+$/.test(m)) return { kind: "minutes", every: +m.slice(2) };
  }
  return { kind: "custom", cron };
}

export function toCron(r: Repeat): string {
  const hm = (t: string) => {
    const [h, m] = t.split(":").map((x) => +x || 0);
    return `${m} ${h}`;
  };
  switch (r.kind) {
    case "daily":
      return `${hm(r.time)} * * *`;
    case "weekdays":
      return `${hm(r.time)} * * 1-5`;
    case "weekly":
      return `${hm(r.time)} * * ${(r.days.length ? r.days : [1]).join(",")}`;
    case "hours":
      return `0 */${r.every} * * *`;
    case "minutes":
      return `*/${r.every} * * * *`;
    case "custom":
      return r.cron;
  }
}

export function describe(cron: string): string {
  const r = fromCron(cron);
  switch (r.kind) {
    case "daily":
      return `Every day at ${r.time}`;
    case "weekdays":
      return `Weekdays at ${r.time}`;
    case "weekly":
      return `Every ${r.days.map((d) => DAY_NAMES[d]).join(", ")} at ${r.time}`;
    case "hours":
      return r.every === 1 ? "Every hour" : `Every ${r.every} hours`;
    case "minutes":
      return `Every ${r.every} minutes`;
    case "custom":
      return `Schedule "${r.cron}"`;
  }
}

const norm = (p: string) => shortPath(p.trim()).replace(/[\\/]+$/, "");

const shortPath = (p: string) => p.replace(/^\/(home|Users)\/[^/]+/, "~").replace(/^[A-Z]:\\Users\\[^\\]+/, "~");

function when(ms: number | null) {
  if (!ms) return "";
  const d = new Date(ms);
  const same = d.toDateString() === new Date().toDateString();
  return same ? d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : d.toLocaleString([], { weekday: "short", hour: "2-digit", minute: "2-digit" });
}

// ---------- The editor ----------

export function TriggerEditor({ settings, value, onChange }: { settings: Settings; value: Trigger[]; onChange: (v: Trigger[]) => void }) {
  const [status, setStatus] = useState<TriggerStatus[]>([]);
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [groups, setGroups] = useState<ToolGroup[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  const [message, setMessage] = useState<{ id: string; text: string } | null>(null);

  useEffect(() => {
    const load = () => void api.triggersStatus().then(setStatus, () => {});
    load();
    const off = listen("triggers://changed", load);
    return () => void off.then((f) => f());
  }, [value]);
  useEffect(() => {
    api.templates().then(setTemplates, () => {});
    api.toolGroups().then(setGroups, () => {});
  }, [settings.agents.templates]);

  const add = () => {
    const t: Trigger = {
      id: `g${Date.now()}`,
      name: "Morning briefing",
      enabled: false,
      when: { type: "schedule", cron: "45 8 * * 1-5" },
      template: templates.some((x) => x.template.id === "morning-briefing") ? "morning-briefing" : "",
      values: {},
      goal: "",
      tools: [],
    };
    onChange([...value, t]);
    setOpen(t.id);
  };

  return (
    <div className="providers">
      {value.length === 0 && (
        <div className="providers__empty">
          <p>
            <strong>Nothing starts by itself yet.</strong> A trigger starts agents on a schedule ("weekdays at 8:45") or when new files
            land in a folder ("sort new downloads"). Each runs at most 10 times an hour and pauses itself after 3 failures in a row.
            Triggers only run while Helpy is running.
          </p>
        </div>
      )}
      {value.map((t) => {
        const st = status.find((s) => s.id === t.id);
        const rt = st?.runtime;
        const summary =
          t.when.type === "schedule" ? describe(t.when.cron) : `When files arrive in ${shortPath(t.when.path) || "a folder"}${t.when.pattern ? ` (${t.when.pattern})` : ""}`;
        const state = rt?.paused
          ? { cls: "pill--err", text: "Paused" }
          : !t.enabled
            ? { cls: "pill--idle", text: "Off" }
            : rt?.openBatch
              ? { cls: "pill--ok", text: "Running" }
              : { cls: "pill--ok", text: st?.nextRun ? `Next ${when(st.nextRun)}` : "Watching" };
        return (
          <div key={t.id} className={`pcard${open === t.id ? " is-open" : ""}`}>
            <div className="pcard__head">
              <span className="pmark" aria-hidden="true">
                {t.when.type === "schedule" ? "⏱" : "⌂"}
              </span>
              <div className="pcard__title">
                <strong>{t.name}</strong>
                <span className="muted">{summary}</span>
              </div>
              <span className={`pill ${state.cls}`}>{state.text}</span>
              <Toggle id={`trig-on-${t.id}`} checked={t.enabled} onChange={(enabled) => onChange(value.map((x) => (x.id === t.id ? { ...x, enabled } : x)))} />
              <button
                type="button"
                className="btn btn--ghost btn--sm"
                onClick={() =>
                  api.triggerRunNow(t.id).then(
                    () => setMessage({ id: t.id, text: "Started. It's in the agent panel." }),
                    (e) => setMessage({ id: t.id, text: String(e) }),
                  )
                }
              >
                Run now
              </button>
              <button type="button" className="btn btn--ghost btn--sm" onClick={() => setOpen(open === t.id ? null : t.id)}>
                {open === t.id ? "Done" : "Edit"}
              </button>
            </div>
            {rt?.paused && (
              <p className="pcard__test is-err">
                {rt.paused}{" "}
                <button type="button" className="link-btn" onClick={() => api.triggerResume(t.id)}>
                  Resume
                </button>
              </p>
            )}
            {st?.problem && <p className="pcard__test is-err">{st.problem}</p>}
            {!rt?.paused && rt?.lastOutcome && rt.lastRun && (
              <p className="pcard__note">
                Last run {when(rt.lastRun)}: {rt.lastOutcome}
              </p>
            )}
            {message?.id === t.id && <p className="pcard__note">{message.text}</p>}
            {open === t.id && (
              <TriggerForm
                trigger={t}
                settings={settings}
                templates={templates}
                groups={groups}
                onChange={(n) => onChange(value.map((x) => (x.id === t.id ? n : x)))}
                onRemove={() => onChange(value.filter((x) => x.id !== t.id))}
              />
            )}
          </div>
        );
      })}
      <button type="button" className="btn btn--ghost add-btn" onClick={add}>
        + New trigger
      </button>
    </div>
  );
}

function TriggerForm(props: {
  trigger: Trigger;
  settings: Settings;
  templates: TemplateInfo[];
  groups: ToolGroup[];
  onChange: (t: Trigger) => void;
  onRemove: () => void;
}) {
  const { trigger: t, settings, templates, groups, onChange } = props;
  const [name, setName] = useState(t.name);
  const [goal, setGoal] = useState(t.goal);
  const [cron, setCron] = useState(t.when.type === "schedule" ? t.when.cron : "");
  const [pattern, setPattern] = useState(t.when.type === "folder" ? t.when.pattern : "");
  useEffect(() => setName(t.name), [t.name]);
  const template = templates.find((x) => x.template.id === t.template)?.template;
  const repeat = t.when.type === "schedule" ? fromCron(t.when.cron) : null;
  const setRepeat = (r: Repeat) => onChange({ ...t, when: { type: "schedule", cron: toCron(r) } });

  // Folders the chosen template works in that agents can't use yet.
  const approved = settings.agents.approvedFolders;
  const needs = (template?.folders ?? [])
    .map((f) => f.replace(/\{(\w+)\}/g, (_, k) => t.values[k] ?? template?.params.find((p) => p.key === k)?.default ?? ""))
    .filter((f) => {
      const want = norm(f);
      return want && !approved.map(norm).some((a) => want === a || want.startsWith(`${a}/`) || want.startsWith(`${a}\\`));
    });

  return (
    <div className="pform">
      <label className="field">
        <span>Name</span>
        <input value={name} onChange={(e) => setName(e.target.value)} onBlur={() => name.trim() && name !== t.name && onChange({ ...t, name: name.trim() })} />
      </label>

      <div className="crow">
        <div className="rules__label">Starts</div>
        <Segmented
          id={`trig-kind-${t.id}`}
          value={t.when.type}
          options={[
            { value: "schedule", label: "On a schedule" },
            { value: "folder", label: "When files arrive" },
          ]}
          onChange={(v) =>
            onChange({ ...t, when: v === "schedule" ? { type: "schedule", cron: "0 9 * * *" } : { type: "folder", path: "", pattern: "" } })
          }
        />
      </div>

      {repeat && (
        <div className="trig-when">
          <select
            value={repeat.kind}
            onChange={(e) => {
              const k = e.target.value as Repeat["kind"];
              const time = "time" in repeat ? repeat.time : "09:00";
              setRepeat(
                k === "daily" || k === "weekdays"
                  ? { kind: k, time }
                  : k === "weekly"
                    ? { kind: k, time, days: [1] }
                    : k === "hours"
                      ? { kind: k, every: 2 }
                      : k === "minutes"
                        ? { kind: k, every: 30 }
                        : { kind: "custom", cron: toCron(repeat) },
              );
            }}
          >
            <option value="daily">Every day</option>
            <option value="weekdays">Weekdays</option>
            <option value="weekly">On these days</option>
            <option value="hours">Every few hours</option>
            <option value="minutes">Every few minutes</option>
            <option value="custom">Custom (cron)</option>
          </select>
          {"time" in repeat && (
            <input type="time" value={repeat.time} onChange={(e) => e.target.value && setRepeat({ ...repeat, time: e.target.value })} />
          )}
          {repeat.kind === "weekly" && (
            <div className="chips">
              {DAY_NAMES.map((d, i) => {
                const on = repeat.days.includes(i);
                return (
                  <button
                    key={d}
                    type="button"
                    className="chip"
                    role="checkbox"
                    aria-checked={on}
                    onClick={() => setRepeat({ ...repeat, days: on ? repeat.days.filter((x) => x !== i) : [...repeat.days, i].sort() })}
                  >
                    {d}
                  </button>
                );
              })}
            </div>
          )}
          {repeat.kind === "hours" && (
            <select value={repeat.every} onChange={(e) => setRepeat({ kind: "hours", every: +e.target.value })}>
              {[1, 2, 3, 4, 6, 8, 12].map((n) => (
                <option key={n} value={n}>
                  every {n} {n === 1 ? "hour" : "hours"}
                </option>
              ))}
            </select>
          )}
          {repeat.kind === "minutes" && (
            <select value={repeat.every} onChange={(e) => setRepeat({ kind: "minutes", every: +e.target.value })}>
              {[10, 15, 20, 30].map((n) => (
                <option key={n} value={n}>
                  every {n} minutes
                </option>
              ))}
            </select>
          )}
          {repeat.kind === "custom" && (
            <input
              className="mono"
              value={cron}
              placeholder="minute hour day month weekday"
              onChange={(e) => setCron(e.target.value)}
              onBlur={() => cron !== toCron(repeat) && setRepeat({ kind: "custom", cron: cron.trim() })}
            />
          )}
        </div>
      )}

      {t.when.type === "folder" && (
        <div className="pform__grid">
          <div className="field">
            <span>Folder to watch</span>
            <FolderPicker
              value={t.when.path}
              placeholder="Choose a folder"
              onChange={(path) => {
                // A template with a folder blank works on the watched folder.
                const blank = template?.params.find((p) => p.kind === "folder");
                onChange({ ...t, when: { type: "folder", path, pattern }, values: blank ? { ...t.values, [blank.key]: path } : t.values });
              }}
            />
          </div>
          <label className="field">
            <span>Only these files (optional)</span>
            <input
              className="mono"
              value={pattern}
              placeholder="*.pdf, *.png"
              onChange={(e) => setPattern(e.target.value)}
              onBlur={() => t.when.type === "folder" && pattern !== t.when.pattern && onChange({ ...t, when: { ...t.when, pattern } })}
            />
          </label>
        </div>
      )}

      <div className="crow">
        <div className="rules__label">Does</div>
        <select className="trig-what" value={t.template} onChange={(e) => onChange({ ...t, template: e.target.value, values: {} })}>
          <option value="">My own task…</option>
          {templates.map((x) => (
            <option key={x.template.id} value={x.template.id}>
              {x.template.name}
            </option>
          ))}
        </select>
      </div>

      {template ? (
        template.params.map((p) => (
          <label key={p.key} className="field">
            <span>{p.label}</span>
            <input
              defaultValue={t.values[p.key] ?? p.default}
              key={`${t.template}-${p.key}-${t.values[p.key] ?? ""}`}
              onBlur={(e) => e.target.value !== (t.values[p.key] ?? p.default) && onChange({ ...t, values: { ...t.values, [p.key]: e.target.value } })}
            />
          </label>
        ))
      ) : (
        <>
          <label className="field">
            <span>What the agent should do</span>
            <div className="textarea">
              <textarea rows={3} value={goal} onChange={(e) => setGoal(e.target.value)} onBlur={() => goal !== t.goal && onChange({ ...t, goal })} />
            </div>
          </label>
          <div className="chips">
            {groups.map((g) => {
              const on = t.tools.includes(g.id);
              return (
                <button
                  key={g.id}
                  type="button"
                  className="chip"
                  role="checkbox"
                  aria-checked={on}
                  onClick={() => onChange({ ...t, tools: on ? t.tools.filter((x) => x !== g.id) : [...t.tools, g.id] })}
                >
                  {g.label}
                </button>
              );
            })}
          </div>
        </>
      )}

      {needs.length > 0 && (
        <p className="pcard__test is-err">
          Agents can't use {needs.map(shortPath).join(", ")} yet, so this won't run.{" "}
          <button type="button" className="link-btn" onClick={() => api.set("agents.approvedFolders", [...approved, ...needs])}>
            Let agents use it
          </button>
        </p>
      )}

      <div className="pform__foot">
        <button type="button" className="btn btn--ghost" onClick={props.onRemove}>
          Delete trigger
        </button>
      </div>
    </div>
  );
}
