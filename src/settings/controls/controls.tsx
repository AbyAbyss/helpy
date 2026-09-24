import { useEffect, useId, useState, type ReactNode } from "react";
import { useCtx } from "../context";

/* ------------------------------------------------------------------ Toggle */

export function Toggle({ path, disabled }: { path: string; disabled?: boolean }) {
  const { store } = useCtx();
  const on = store.get<boolean>(path);
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      disabled={disabled}
      className="ctl-toggle"
      onClick={() => store.set(path, !on)}
    >
      <span className="ctl-toggle-knob" />
    </button>
  );
}

/* ------------------------------------------------------------------ Segmented */

export interface Option<T extends string> {
  value: T;
  label: string;
}

export function Segmented<T extends string>({ path, options }: { path: string; options: Option<T>[] }) {
  const { store } = useCtx();
  const value = store.get<T>(path);
  return (
    <div className="ctl-seg" role="radiogroup">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          className="ctl-seg-item"
          onClick={() => store.set(path, o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/* ------------------------------------------------------------------ Select */

export function Select<T extends string>({ path, options }: { path: string; options: Option<T>[] }) {
  const { store } = useCtx();
  const id = useId();
  return (
    <div className="ctl-select">
      <select id={id} value={store.get<T>(path)} onChange={(e) => store.set(path, e.target.value)}>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <svg viewBox="0 0 12 12" aria-hidden="true">
        <path d="M3 4.5 6 7.5 9 4.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </div>
  );
}

/* ------------------------------------------------------------------ Slider */

export function Slider({
  path,
  min,
  max,
  step = 1,
  format,
  ends,
}: {
  path: string;
  min: number;
  max: number;
  step?: number;
  format: (v: number) => string;
  ends?: [string, string];
}) {
  const { store } = useCtx();
  const value = store.get<number>(path);
  const pct = ((value - min) / (max - min)) * 100;
  return (
    <div className="ctl-slider">
      <div className="ctl-slider-track">
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          style={{ "--pct": `${pct}%` } as React.CSSProperties}
          onChange={(e) => store.set(path, Number(e.target.value))}
        />
        {ends && (
          <div className="ctl-slider-ends">
            <span>{ends[0]}</span>
            <span>{ends[1]}</span>
          </div>
        )}
      </div>
      <output className="ctl-value">{format(value)}</output>
    </div>
  );
}

/* ------------------------------------------------------------------ Number */

/** Text input for numbers. Commits on blur or Enter so half-typed values don't flash errors. */
export function NumberField({ path, min, max, suffix }: { path: string; min: number; max: number; suffix?: string }) {
  const { store } = useCtx();
  const value = store.get<number>(path);
  const [draft, setDraft] = useState(String(value));
  const [local, setLocal] = useState<string | null>(null);
  useEffect(() => setDraft(String(value)), [value]);

  const commit = () => {
    const n = Number(draft);
    if (draft.trim() === "" || !Number.isFinite(n) || !Number.isInteger(n)) {
      setLocal("Enter a whole number.");
      return;
    }
    if (n < min || n > max) {
      setLocal(`Use a value from ${min} to ${max}.`);
      return;
    }
    setLocal(null);
    if (n !== value) store.set(path, n);
  };

  return (
    <div className="ctl-number-wrap">
      <div className="ctl-number" data-invalid={local ? "yes" : "no"}>
        <input
          inputMode="numeric"
          value={draft}
          aria-invalid={!!local}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()}
        />
        {suffix && <span>{suffix}</span>}
      </div>
      {local && <p className="row-error">{local}</p>}
    </div>
  );
}

/* ------------------------------------------------------------------ Button */

export function Button({
  children,
  onClick,
  variant = "secondary",
  disabled,
  icon,
}: {
  children: ReactNode;
  onClick?: () => void;
  variant?: "primary" | "secondary" | "ghost" | "danger";
  disabled?: boolean;
  icon?: ReactNode;
}) {
  return (
    <button type="button" className={`btn btn-${variant}`} onClick={onClick} disabled={disabled}>
      {icon}
      {children}
    </button>
  );
}
