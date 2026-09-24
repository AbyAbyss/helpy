import { useEffect, useId, useState } from "react";
import { acceleratorFromEvent, keycaps, modifiersOf } from "../lib/keys";

export function Toggle({ id, checked, onChange }: { id: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      className="toggle"
      onClick={() => onChange(!checked)}
    >
      <span className="toggle__knob" />
    </button>
  );
}

type Option = { value: string; label: string };

export function Segmented({ id, value, options, onChange }: { id: string; value: string; options: Option[]; onChange: (v: string) => void }) {
  return (
    <div id={id} className="segmented" role="radiogroup">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          className="segmented__item"
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Select({ id, value, options, onChange }: { id: string; value: string; options: Option[]; onChange: (v: string) => void }) {
  return (
    <div className="select">
      <select id={id} value={value} onChange={(e) => onChange(e.target.value)} disabled={options.length < 2}>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <svg className="select__chevron" viewBox="0 0 12 12" aria-hidden="true">
        <path d="M3 4.5 6 7.5 9 4.5" />
      </svg>
    </div>
  );
}

export function Slider(props: {
  id: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (v: number) => string;
  ends?: [string, string];
  onChange: (v: number) => void;
}) {
  const { id, value, min, max, step, format, ends, onChange } = props;
  const fill = ((value - min) / (max - min)) * 100;
  return (
    <div className="slider">
      <div className="slider__track-row">
        {ends && <span className="slider__end">{ends[0]}</span>}
        <input
          id={id}
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          style={{ "--fill": `${fill}%` } as React.CSSProperties}
          onChange={(e) => onChange(Number(e.target.value))}
        />
        {ends && <span className="slider__end">{ends[1]}</span>}
      </div>
      <output htmlFor={id} className="slider__value">
        {format(value)}
      </output>
    </div>
  );
}

/** Number input that keeps what you type until it's a valid number. */
export function NumberField(props: { id: string; value: number; min: number; max: number; unit: string; invalid: boolean; onChange: (v: number) => void }) {
  const { id, value, min, max, unit, invalid, onChange } = props;
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  return (
    <label className={`number${invalid ? " is-invalid" : ""}`}>
      <input
        id={id}
        inputMode="numeric"
        value={text}
        aria-invalid={invalid}
        onChange={(e) => {
          setText(e.target.value);
          const n = Number(e.target.value);
          if (e.target.value.trim() !== "" && Number.isInteger(n)) onChange(n);
        }}
        onBlur={() => {
          if (!/^-?\d+$/.test(text.trim())) setText(String(value));
        }}
        onKeyDown={(e) => {
          const delta = e.key === "ArrowUp" ? 1 : e.key === "ArrowDown" ? -1 : 0;
          if (!delta) return;
          e.preventDefault();
          onChange(Math.min(max, Math.max(min, value + delta * (e.shiftKey ? 10 : 1))));
        }}
      />
      <span className="number__unit">{unit}</span>
    </label>
  );
}

export function Keycaps({ accel, muted }: { accel: string; muted?: boolean }) {
  const caps = keycaps(accel);
  if (!caps.length) return <span className="keycaps keycaps--empty">Not set</span>;
  return (
    <span className={`keycaps${muted ? " keycaps--muted" : ""}`}>
      {caps.map((k, i) => (
        <kbd key={i}>{k}</kbd>
      ))}
    </span>
  );
}

/**
 * Click to record a new combination. Esc cancels, Backspace unbinds.
 * Helpy's own hotkeys are suspended while recording so they reach this field.
 */
export function HotkeyField(props: {
  id: string;
  value: string;
  invalid: boolean;
  onRecordingChange: (recording: boolean) => void;
  onChange: (accel: string) => void;
}) {
  const { id, value, invalid, onRecordingChange, onChange } = props;
  const [recording, setRecording] = useState(false);
  const [held, setHeld] = useState<string[]>([]);
  const hintId = useId();

  const stop = () => {
    setRecording(false);
    setHeld([]);
    onRecordingChange(false);
  };

  return (
    <button
      id={id}
      type="button"
      className={`hotkey${recording ? " is-recording" : ""}${invalid ? " is-invalid" : ""}`}
      aria-describedby={hintId}
      onClick={() => {
        if (recording) return;
        setRecording(true);
        onRecordingChange(true);
      }}
      onBlur={() => recording && stop()}
      onKeyDown={(e) => {
        if (!recording) return;
        e.preventDefault();
        e.stopPropagation();
        const bare = !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey;
        if (bare && e.key === "Escape") return stop();
        if (bare && (e.key === "Backspace" || e.key === "Delete")) {
          onChange("");
          return stop();
        }
        const accel = acceleratorFromEvent(e.nativeEvent);
        if (!accel) return setHeld(modifiersOf(e.nativeEvent));
        onChange(accel);
        stop();
      }}
      onKeyUp={(e) => recording && setHeld(modifiersOf(e.nativeEvent))}
    >
      {recording ? (
        held.length ? <Keycaps accel={[...held, ""].join("+").replace(/\+$/, "+…")} muted /> : <span className="hotkey__prompt">Press keys…</span>
      ) : (
        <Keycaps accel={value} />
      )}
      <span id={hintId} className="sr-only">
        {recording ? "Press a key combination. Escape cancels, Backspace removes the hotkey." : "Click to change"}
      </span>
    </button>
  );
}
