import type { Ai } from "../../bindings/Ai";
import type { ModelRef } from "../../bindings/ModelRef";
import type { Routing } from "../../bindings/Routing";
import { allModels, refKey, ROUTES } from "./routing";

function ModelSelect(props: { id: string; ai: Ai; value: ModelRef | null; onChange: (r: ModelRef | null) => void; emptyLabel: string; exclude?: ModelRef[] }) {
  const { id, ai, value, onChange, emptyLabel, exclude = [] } = props;
  const models = allModels(ai).filter((m) => !exclude.some((x) => refKey(x) === refKey(m.ref)));
  return (
    <div className="select">
      <select
        id={id}
        value={value ? refKey(value) : ""}
        onChange={(e) => onChange(models.find((m) => refKey(m.ref) === e.target.value)?.ref ?? null)}
      >
        <option value="">{emptyLabel}</option>
        {ai.providers.map((p) => (
          <optgroup key={p.id} label={p.name}>
            {models
              .filter((m) => m.ref.providerId === p.id)
              .map((m) => (
                <option key={refKey(m.ref)} value={refKey(m.ref)}>
                  {m.model.id}
                  {m.model.vision ? " · sees images" : ""}
                </option>
              ))}
          </optgroup>
        ))}
      </select>
      <svg className="select__chevron" viewBox="0 0 12 12" aria-hidden="true">
        <path d="M3 4.5 6 7.5 9 4.5" />
      </svg>
    </div>
  );
}

export function RoutingEditor({ ai, onChange }: { ai: Ai; onChange: (r: Routing) => void }) {
  if (allModels(ai).length === 0) return <p className="muted">Add a provider with at least one model first.</p>;
  return (
    <div className="routes">
      {ROUTES.map((r) => (
        <div key={r.key} className="route">
          <label htmlFor={`route-${r.key}`} className="route__label">
            {r.label}
            {!r.built && <span className="pill pill--idle">Not built yet</span>}
            {r.help && <span className="route__help">{r.help}</span>}
          </label>
          <ModelSelect
            id={`route-${r.key}`}
            ai={ai}
            value={ai.routing[r.key]}
            emptyLabel="Not set"
            onChange={(v) => onChange({ ...ai.routing, [r.key]: v })}
          />
        </div>
      ))}
    </div>
  );
}

export function FallbackEditor({ ai, onChange }: { ai: Ai; onChange: (chain: ModelRef[]) => void }) {
  const chain = ai.fallbackChain;
  const label = (r: ModelRef) => {
    const m = allModels(ai).find((x) => refKey(x.ref) === refKey(r));
    return m ? `${m.model.id} · ${m.provider}` : r.model;
  };
  const move = (i: number, by: number) => {
    const next = [...chain];
    [next[i], next[i + by]] = [next[i + by], next[i]];
    onChange(next);
  };
  return (
    <div className="chain">
      {chain.length > 0 && (
        <ol className="chain__list">
          {chain.map((r, i) => (
            <li key={refKey(r)} className="chain__item">
              <span className="chain__n">{i + 1}</span>
              <span className="mono chain__name">{label(r)}</span>
              <button type="button" className="icon-btn" aria-label="Move up" disabled={i === 0} onClick={() => move(i, -1)}>↑</button>
              <button type="button" className="icon-btn" aria-label="Move down" disabled={i === chain.length - 1} onClick={() => move(i, 1)}>↓</button>
              <button type="button" className="icon-btn" aria-label={`Remove ${label(r)}`} onClick={() => onChange(chain.filter((_, j) => j !== i))}>×</button>
            </li>
          ))}
        </ol>
      )}
      <ModelSelect id="fallback-add" ai={ai} value={null} exclude={chain} emptyLabel={chain.length ? "Add another fallback…" : "Add a fallback…"} onChange={(r) => r && onChange([...chain, r])} />
    </div>
  );
}
