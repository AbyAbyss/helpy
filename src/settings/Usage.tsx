import { useEffect, useState } from "react";
import type { Total } from "../bindings/Total";
import type { UsageHistory } from "../bindings/UsageHistory";
import { api } from "../lib/ipc";

type Metric = "tokens" | "cost";

const shortTokens = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : String(n));
const money = (n: number) => `$${n < 10 ? n.toFixed(2) : n.toFixed(0)}`;
const dayLabel = (date: string) => new Date(`${date}T12:00`).toLocaleDateString(undefined, { day: "numeric", month: "short" });

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

function Breakdown({ title, rows, value, format }: { title: string; rows: Total[]; value: (t: Total) => number; format: (n: number) => string }) {
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
          <span className="uhist__calls">
            {r.calls} {r.calls === 1 ? "call" : "calls"}
          </span>
        </div>
      ))}
    </div>
  );
}
