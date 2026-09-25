import { useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import Markdown from "react-markdown";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { CircleAction } from "../bindings/CircleAction";
import type { CircleEvent } from "../bindings/CircleEvent";
import type { LabelPart } from "../bindings/LabelPart";
import type { SelectionShape } from "../bindings/SelectionShape";
import { api, EVENTS } from "../lib/ipc";
import { apply, idle, running, type CircleView, type Turn } from "./circleState";
import { boundsOf, nearestOnRect, placeCard, placeLabels, type Pt, type Rect } from "./placement";
import "./circle.css";

const CARD = { w: 400, h: 440 };
const ACTIONS: { id: Exclude<CircleAction, "menu">; label: string }[] = [
  { id: "explain", label: "Explain" },
  { id: "copyText", label: "Copy text" },
  { id: "translate", label: "Translate" },
  { id: "summarize", label: "Summarize" },
];
const TITLES: Record<CircleAction, string> = {
  explain: "Explanation",
  copyText: "Text",
  translate: "Translation",
  summarize: "Summary",
  menu: "",
};

/**
 * Circle to explain on one overlay. While it's open the overlay takes the
 * mouse; everything here goes away (and the overlay is click-through again)
 * when it closes.
 */
export function CircleLayer({ color, onOpen }: { color: string; onOpen: (open: boolean) => void }) {
  const self = useMemo(() => getCurrentWebviewWindow().label, []);
  const [view, setView] = useState<CircleView>(idle);
  const [shape, setShape] = useState<SelectionShape>("rectangle");
  const [points, setPoints] = useState<Pt[]>([]);
  const [hint, setHint] = useState<string | null>(null);
  const [part, setPart] = useState<number | null>(null);
  const drawing = useRef<{ start: Pt } | null>(null);

  useEffect(() => {
    const off = listen<CircleEvent>(EVENTS.circle, ({ payload }) => {
      if (payload.type === "select" || payload.type === "closed") {
        setShape(payload.type === "select" ? payload.shape : "rectangle");
        setPoints([]);
        setHint(null);
        setPart(null);
      }
      setView((s) => apply(s, payload, self));
    });
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && api.circleClose();
    window.addEventListener("keydown", onKey);
    return () => {
      off.then((f) => f());
      window.removeEventListener("keydown", onKey);
    };
  }, [self]);

  useEffect(() => onOpen(view.phase !== "idle"), [view.phase, onOpen]);

  if (view.phase === "idle") return null;

  const bounds = { w: window.innerWidth, h: window.innerHeight };
  const outline = shape === "rectangle" && points.length === 2 ? corners(points[0], points[1]) : points;
  const sel = outline.length > 1 ? boundsOf(outline) : null;

  const down = (e: ReactPointerEvent) => {
    if (view.phase !== "select" || view.problem || e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    const p = { x: e.clientX, y: e.clientY };
    drawing.current = { start: p };
    setHint(null);
    setPoints([p]);
  };
  const move = (e: ReactPointerEvent) => {
    const d = drawing.current;
    if (!d) return;
    const p = { x: e.clientX, y: e.clientY };
    if (shape === "rectangle") setPoints([d.start, p]);
    else setPoints((pts) => (dist(pts[pts.length - 1], p) > 3 ? [...pts, p] : pts));
  };
  const up = () => {
    if (!drawing.current) return;
    drawing.current = null;
    const b = outline.length > 1 ? boundsOf(outline) : null;
    if (!b || b.w < 8 || b.h < 8) {
      setPoints([]);
      setHint("Drag around something to select it");
      return;
    }
    setPoints(outline);
    setView((s) => ({ ...s, phase: "result" }));
    api.circleSelect(outline.map((p) => [p.x, p.y] as [number, number])).catch((err) => {
      setPoints([]);
      setView((s) => ({ ...s, phase: "select" }));
      setHint(String(err));
    });
  };

  return (
    <div
      className={`circle circle--${view.phase}`}
      style={{ ["--mark" as string]: color }}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
    >
      <svg className="circle__svg" width={bounds.w} height={bounds.h}>
        <path
          className="circle__scrim"
          d={scrim(bounds, outline)}
          fillRule="evenodd"
          // Outside the selection closes the result; the hole lets clicks through to the root.
          onClick={() => view.phase === "result" && api.circleClose()}
        />
        {outline.length > 1 && <path className="circle__outline" d={pathOf(outline, shape === "rectangle" || view.phase === "result")} />}
      </svg>

      {view.phase === "select" && (
        <Toolbar
          shape={shape}
          problem={view.problem}
          hint={hint}
          onShape={(s) => {
            setShape(s);
            setPoints([]);
          }}
        />
      )}

      {view.phase === "result" && sel && <Result view={view} sel={sel} bounds={bounds} part={part} onPart={setPart} />}
    </div>
  );
}

function Toolbar({ shape, problem, hint, onShape }: { shape: SelectionShape; problem: string | null; hint: string | null; onShape: (s: SelectionShape) => void }) {
  return (
    <div className="circle-bar" onPointerDown={(e) => e.stopPropagation()}>
      {problem ? (
        <>
          <span className="circle-bar__problem">{problem}</span>
          <button
            type="button"
            className="cbtn cbtn--primary"
            onClick={() => {
              api.circleClose();
              api.openSettingsSection("ai");
            }}
          >
            Open settings
          </button>
        </>
      ) : (
        <>
          <div className="seg" role="radiogroup" aria-label="Selection shape">
            <button type="button" role="radio" aria-checked={shape === "rectangle"} onClick={() => onShape("rectangle")}>
              <svg viewBox="0 0 20 20" width="15" height="15" aria-hidden="true">
                <rect x="3" y="4.5" width="14" height="11" rx="2" />
              </svg>
              Box
            </button>
            <button type="button" role="radio" aria-checked={shape === "freehand"} onClick={() => onShape("freehand")}>
              <svg viewBox="0 0 20 20" width="15" height="15" aria-hidden="true">
                <path d="M6 4.5c4-2 10-.5 10.5 4S12 16 8 15.5 2.5 11 4 7.5" />
              </svg>
              Freehand
            </button>
          </div>
          <span className="circle-bar__hint">{hint ?? (shape === "rectangle" ? "Drag a box around anything" : "Draw around anything")}</span>
        </>
      )}
      <button type="button" className="cbtn cbtn--quiet" onClick={() => api.circleClose()}>
        Cancel <kbd>Esc</kbd>
      </button>
    </div>
  );
}

function Result({ view, sel, bounds, part, onPart }: { view: CircleView; sel: Rect; bounds: { w: number; h: number }; part: number | null; onPart: (i: number | null) => void }) {
  const card = placeCard(sel, CARD, bounds);
  const sizes = view.parts.map(labelSize);
  const rects = placeLabels(sel, view.parts, sizes, bounds, [card]);

  return (
    <>
      <svg className="circle__svg circle__leaders" width={bounds.w} height={bounds.h} aria-hidden="true">
        {view.parts.map((p, i) => {
          const r = rects[i];
          const end = r ? nearestOnRect(p, r) : null;
          return (
            <g key={i} className={part === i ? "is-on" : undefined}>
              {end && <line x1={p.x} y1={p.y} x2={end.x} y2={end.y} />}
              <circle cx={p.x} cy={p.y} r={3.5} />
            </g>
          );
        })}
      </svg>
      {view.parts.map((p, i) => {
        const r = rects[i];
        if (!r) return null;
        return (
          <button
            key={i}
            type="button"
            className="clabel"
            aria-pressed={part === i}
            style={{ left: r.x, top: r.y, width: r.w, height: r.h }}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={() => onPart(part === i ? null : i)}
          >
            <span className="clabel__name">{p.label}</span>
            {p.note && <span className="clabel__note">{p.note}</span>}
          </button>
        );
      })}
      <Card view={view} rect={card} part={part == null ? null : view.parts[part]} onBack={() => onPart(null)} />
    </>
  );
}

function Card({ view, rect, part, onBack }: { view: CircleView; rect: Rect; part: LabelPart | null; onBack: () => void }) {
  const [question, setQuestion] = useState("");
  const busy = running(view);
  const body = useRef<HTMLDivElement>(null);
  const last = view.turns[view.turns.length - 1];
  const lastText = last?.text;

  // Keep the newest answer in view as it streams.
  useEffect(() => {
    body.current?.scrollTo({ top: body.current.scrollHeight });
  }, [view.turns.length, lastText]);

  const ask = () => {
    const q = question.trim();
    if (!q || busy) return;
    setQuestion("");
    api.circleAction("explain", q);
  };

  return (
    <section className="ccard" style={{ left: rect.x, top: rect.y, maxHeight: rect.h }} onPointerDown={(e) => e.stopPropagation()} aria-label="Circle to explain">
      <header className="ccard__head">
        <svg viewBox="0 0 64 64" width="18" height="18" aria-hidden="true">
          <path d="M6 6 L26 13.5 A22 22 0 1 1 13.5 26 Z" fill="var(--mark)" />
          <ellipse cx="29" cy="33" rx="3.2" ry="4.4" fill="#141821" />
          <ellipse cx="41" cy="33" rx="3.2" ry="4.4" fill="#141821" />
        </svg>
        <div className="ccard__actions" role="group" aria-label="Actions">
          {ACTIONS.map((a) => (
            <button
              key={a.id}
              type="button"
              className="chipb"
              aria-pressed={!part && last?.action === a.id && !last.question}
              disabled={busy}
              onClick={() => {
                onBack();
                api.circleAction(a.id);
              }}
            >
              {a.label}
            </button>
          ))}
        </div>
        <button type="button" className="ccard__close" aria-label="Close" onClick={() => api.circleClose()}>
          <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
            <path d="m5 5 10 10M15 5 5 15" />
          </svg>
        </button>
      </header>

      <div className="ccard__body" ref={body}>
        {part ? (
          <div className="ccard__part">
            <button type="button" className="ccard__back" onClick={onBack}>
              ‹ Back
            </button>
            <h3>{part.label}</h3>
            <p>{part.detail ?? part.note ?? "Helpy didn't say more about this part."}</p>
          </div>
        ) : view.turns.length === 0 ? (
          <p className="ccard__empty">What should Helpy do with this?</p>
        ) : (
          view.turns.map((t, i) => <TurnView key={i} turn={t} />)
        )}
      </div>

      <form
        className="ccard__ask"
        onSubmit={(e) => {
          e.preventDefault();
          ask();
        }}
      >
        <input
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          placeholder="Ask about this, or give an agent a task…"
          aria-label="Ask about this, or give an agent a task"
          autoFocus
        />
        <button
          type="button"
          className="ccard__agent"
          disabled={!question.trim()}
          title="Send to an agent, with this selection"
          onClick={() => api.circleToAgent(question).catch(() => {})}
        >
          To agent
        </button>
        <button type="submit" className="ccard__send" disabled={busy || !question.trim()} aria-label="Ask">
          <svg viewBox="0 0 20 20" width="15" height="15" aria-hidden="true">
            <path d="M10 15.5v-11M5.5 9 10 4.5 14.5 9" />
          </svg>
        </button>
      </form>
    </section>
  );
}

function TurnView({ turn: t }: { turn: Turn }) {
  const copyable = t.status === "done" && t.text && t.copied == null && (t.action === "translate" || t.action === "summarize" || t.question);
  return (
    <article className="turn">
      <h4 className="turn__title">{t.question ? "You asked" : TITLES[t.action]}</h4>
      {t.question && <p className="turn__q">{t.question}</p>}
      {t.status === "working" && !t.text && (
        <p className="turn__wait">
          <span className="spinner" aria-hidden="true" />
          {t.retry ?? (t.action === "copyText" ? "Reading the text…" : "Looking…")}
        </p>
      )}
      {t.text &&
        (t.action === "copyText" && !t.question ? (
          <pre className="turn__text">{t.text}</pre>
        ) : (
          <div className="turn__md">
            <Markdown components={{ a: ({ children }) => <span>{children}</span>, img: () => null }}>{t.text}</Markdown>
          </div>
        ))}
      {t.copied != null && <p className="turn__ok">Copied {t.copied.toLocaleString()} characters to the clipboard.</p>}
      {copyable && (
        <button type="button" className="cbtn cbtn--small" onClick={() => api.circleCopy(t.text)}>
          Copy
        </button>
      )}
      {t.error && (
        <div className="turn__err" role="alert">
          <p>{t.error.message}</p>
          {t.error.action && (
            <button
              type="button"
              className="cbtn cbtn--small"
              onClick={() => {
                api.circleClose();
                api.openSettingsSection(t.error?.action === "openLimits" ? "usage" : "ai");
              }}
            >
              {t.error.action === "openLimits" ? "Open limits" : "Open AI providers"}
            </button>
          )}
        </div>
      )}
    </article>
  );
}

// ---------- geometry helpers ----------

const dist = (a: Pt, b: Pt) => Math.hypot(a.x - b.x, a.y - b.y);

function corners(a: Pt, b: Pt): Pt[] {
  return [a, { x: b.x, y: a.y }, b, { x: a.x, y: b.y }];
}

function pathOf(pts: Pt[], closed: boolean): string {
  const d = pts.map((p, i) => `${i ? "L" : "M"}${p.x.toFixed(1)} ${p.y.toFixed(1)}`).join("");
  return closed ? `${d}Z` : d;
}

/** The dim layer, with the selection cut out once there is one. */
function scrim(b: { w: number; h: number }, outline: Pt[]): string {
  const all = `M0 0H${b.w}V${b.h}H0Z`;
  return outline.length > 2 ? all + pathOf(outline, true) : all;
}

let ruler: CanvasRenderingContext2D | null = null;

/** Label size from its text, measured in the label's own fonts. */
function labelSize(p: LabelPart): { w: number; h: number } {
  ruler ??= document.createElement("canvas").getContext("2d");
  const measure = (s: string, font: string) => {
    if (!ruler) return s.length * 7.5;
    ruler.font = font;
    return ruler.measureText(s).width;
  };
  // Canvas only takes the standard weights, so 600 stands in for 650.
  const name = measure(p.label, '600 12.5px "Figtree Variable", sans-serif');
  const note = p.note ? measure(p.note, '500 11px "Figtree Variable", sans-serif') : 0;
  // Padding, the accent edge, and a little slack for font fallback.
  return { w: Math.min(Math.ceil(Math.max(name, note) + 28), 260), h: p.note ? 44 : 28 };
}
