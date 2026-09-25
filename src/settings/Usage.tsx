import { useEffect, useState } from "react";
import type { Settings } from "../bindings/Settings";
import type { Total } from "../bindings/Total";
import type { UsageHistory } from "../bindings/UsageHistory";
import type { UsageToday } from "../bindings/UsageToday";
import type { Note } from "../bindings/Note";
import { listen } from "@tauri-apps/api/event";
import { api, EVENTS } from "../lib/ipc";

type Metric = "tokens" | "cost";

const shortTokens = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : String(n));
const money = (n: number) => `$${n < 10 ? n.toFixed(2) : n.toFixed(0)}`;
const dayLabel = (date: string) => new Date(`${date}T12:00`).toLocaleDateString(undefined, { day: "numeric", month: "short" });

/**
 * Today's spending in full (calls, tokens in and out, cached input, cost),
 * the meter against the daily limit, what used it, and the 30-day history.
 * Refreshed whenever settings change, which every saved call triggers.
 */
export function UsagePanel({ settings }: { settings: Settings }) {
  const [usage, setUsage] = useState<UsageToday | null>(null);
  const [today, setToday] = useState<UsageHistory | null>(null);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    void api.usageToday().then(setUsage);
    void api.usageHistory(1).then(setToday, () => {});
  }, [settings]);
  if (!usage) return null;
  const limit = settings.limits.dailyTokenBudget;
  const share = limit > 0 ? Math.min(usage.tokens / limit, 1) : 0;
  const cached = usage.cacheReadTokens + usage.cacheWriteTokens;
  const stats: [string, string, string?][] = [
    ["Calls", usage.calls.toLocaleString()],
    ["Input", shortTokens(usage.inputTokens), "tokens sent, not counting cached ones"],
    ["Output", shortTokens(usage.outputTokens), "tokens the models wrote"],
    ["Cached", shortTokens(cached), `${shortTokens(usage.cacheReadTokens)} read from the provider's cache (cheaper), ${shortTokens(usage.cacheWriteTokens)} written to it`],
    ["Cost", usage.cost > 0 ? `$${usage.cost.toFixed(2)}` : "—", usage.cost > 0 ? "priced models only" : "no priced model used yet"],
  ];
  const withUse = (rows: Total[]) => rows.filter((r) => r.tokens > 0);
  return (
    <div className="usage">
      <div className="usage__text">
        <span>Today</span>
        <strong className="mono">{usage.tokens.toLocaleString()}</strong>
        <span>{limit > 0 ? `of ${limit.toLocaleString()} tokens` : "tokens, no limit"}</span>
        <button type="button" className="link-btn usage__more" aria-expanded={open} onClick={() => setOpen(!open)}>
          {open ? "Hide history" : "Last 30 days"}
        </button>
      </div>
      {limit > 0 && (
        <div className={`meter${share > 0.9 ? " meter--hot" : ""}`} role="meter" aria-valuenow={usage.tokens} aria-valuemin={0} aria-valuemax={limit} aria-label="Tokens used today">
          <span style={{ width: `${share * 100}%` }} />
        </div>
      )}
      <dl className="ustats">
        {stats.map(([label, value, help]) => (
          <div key={label} className="ustats__cell" title={help}>
            <dt>{label}</dt>
            <dd className="mono">{value}</dd>
          </div>
        ))}
      </dl>
      {today && (withUse(today.features).length > 0 || withUse(today.models).length > 0) && (
        <div className="uhist__lists">
          <Breakdown title="Today by feature" rows={today.features} value={(t) => t.tokens} format={shortTokens} cost />
          <Breakdown title="Today by model" rows={today.models} value={(t) => t.tokens} format={shortTokens} cost />
        </div>
      )}
      {open && <UsageHistoryView refresh={usage} />}
    </div>
  );
}

/** The last 30 days of AI use, counted on this computer only. */
export function UsageHistoryView({ refresh }: { refresh: unknown }) {
  const [history, setHistory] = useState<UsageHistory | null>(null);
  const [metric, setMetric] = useState<Metric>("tokens");
  const [hover, setHover] = useState<number | null>(null);
  useEffect(() => void api.usageHistory(30).then(setHistory, () => {}), [refresh]);
  if (!history) return null;

  const value = (x: { tokens: number; cost: number }) => (metric === "tokens" ? x.tokens : x.cost);
  const format = (n: number) => (metric === "tokens" ? shortTokens(n) : money(n));
  const max = Math.max(...history.days.map(value), 0);
  const total = history.days.reduce((a, d) => a + value(d), 0);
  const hasCost = history.days.some((d) => d.cost > 0);
  const shown = hover !== null ? history.days[hover] : null;

  if (total === 0 && metric === "tokens")
    return <p className="group__note">No AI use in the last 30 days yet. Usage is counted on this computer and never sent anywhere.</p>;

  return (
    <div className="uhist">
      <div className="uhist__head">
        <div className="uhist__sum">
          <strong className="mono">{format(shown ? value(shown) : total)}</strong>
          <span>{shown ? dayLabel(shown.date) : metric === "tokens" ? "tokens in the last 30 days" : "in the last 30 days (priced models only)"}</span>
        </div>
        {hasCost && (
          <div className="uhist__metric" role="group" aria-label="Show">
            {(["tokens", "cost"] as Metric[]).map((m) => (
              <button key={m} type="button" className={metric === m ? "is-on" : undefined} onClick={() => setMetric(m)}>
                {m === "tokens" ? "Tokens" : "Cost"}
              </button>
            ))}
          </div>
        )}
      </div>
      <div className="uhist__bars" onMouseLeave={() => setHover(null)} role="img" aria-label={`Daily ${metric} for the last 30 days`}>
        {history.days.map((d, i) => (
          <span
            key={d.date}
            className={`uhist__bar${hover === i ? " is-hover" : ""}`}
            style={{ ["--h" as string]: max > 0 ? `${Math.max((value(d) / max) * 100, value(d) > 0 ? 3 : 0)}%` : "0%", ["--i" as string]: i }}
            onMouseEnter={() => setHover(i)}
            title={`${dayLabel(d.date)}: ${format(value(d))}`}
          />
        ))}
      </div>
      <div className="uhist__axis">
        <span>{dayLabel(history.days[0].date)}</span>
        <span>Today</span>
      </div>
      <div className="uhist__lists">
        <Breakdown title="By feature" rows={history.features} value={value} format={format} />
        <Breakdown title="By model" rows={history.models} value={value} format={format} />
      </div>
      <p className="group__note">Counted on this computer and never sent anywhere. Failed attempts count, since providers may bill them.</p>
    </div>
  );
}

function Breakdown({ title, rows, value, format, cost }: { title: string; rows: Total[]; value: (t: Total) => number; format: (n: number) => string; cost?: boolean }) {
  const top = rows.filter((r) => value(r) > 0).slice(0, 5);
  const max = Math.max(...top.map(value), 0);
  if (top.length === 0) return null;
  return (
    <div className="uhist__list">
      <div className="uhist__title">{title}</div>
      {top.map((r) => (
        <div key={r.name} className="uhist__row">
          <span className="uhist__name" title={r.name}>
            {r.name}
          </span>
          <span className="uhist__val mono">{format(value(r))}</span>
          <span className="uhist__track">
            <span style={{ width: `${(value(r) / max) * 100}%` }} />
          </span>
          <span className="uhist__calls" title={`${shortTokens(r.inputTokens)} in · ${shortTokens(r.outputTokens)} out · ${shortTokens(r.cacheReadTokens + r.cacheWriteTokens)} cached`}>
            {r.calls} {r.calls === 1 ? "call" : "calls"}
            {cost && r.cost > 0 && ` · ${money(r.cost)}`}
          </span>
        </div>
      ))}
    </div>
  );
}

/** The notes Helpy has saved about the user, each removable. */
export function MemoryNotes() {
  const [notes, setNotes] = useState<Note[]>([]);
  useEffect(() => {
    const load = () => void api.notes().then(setNotes, () => {});
    load();
    const off = listen(EVENTS.memoryChanged, load);
    return () => void off.then((f) => f());
  }, []);
  if (notes.length === 0) return <p className="group__note">Nothing remembered yet. Tell Helpy about yourself in a conversation and it saves what matters here.</p>;
  return (
    <div className="notes">
      <ul className="notes__list">
        {notes.map((n) => (
          <li key={n.id} className="notes__row">
            <span>{n.text}</span>
            <button type="button" className="link-btn" onClick={() => api.noteDelete(n.id)} aria-label={`Forget "${n.text}"`}>
              Forget
            </button>
          </li>
        ))}
      </ul>
      <button type="button" className="link-btn" onClick={() => api.notesClear()}>
        Forget everything ({notes.length})
      </button>
    </div>
  );
}
