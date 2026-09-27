import { useLayoutEffect, useRef, type CSSProperties } from "react";
import type { Guidance } from "../bindings/Guidance";
import type { Mark } from "../bindings/Mark";
import { arrow, bubbleBelow, dimPath, HIGHLIGHT_PAD, inkFor, labelAbove, lineLabelAt, linePath } from "./geometry";
import "./annotations.css";

type Props = {
  marks: Mark[];
  /** The drawing area in CSS pixels (the monitor, or the settings preview). */
  width: number;
  height: number;
  look: Guidance;
  /**
   * When each mark starts drawing and for how long, ms, so the drawing keeps
   * pace with the buddy touring it. Without it every mark draws at once.
   */
  timing?: { draw: number; dur: number }[];
};

function timed(timing: Props["timing"], i: number): CSSProperties | undefined {
  const t = timing?.[i];
  // The same easing the buddy moves with, so its tip stays on the stroke.
  return t && ({ "--d0": `${t.draw}ms`, "--dd": `${t.dur}ms`, "--ease": "cubic-bezier(0.42, 0, 0.58, 1)" } as CSSProperties);
}

/**
 * Highlights, pointers and arrows for one guidance step. Purely visual: it
 * never takes pointer events, so the app underneath stays usable.
 */
export function Annotations({ marks, width, height, look, timing }: Props) {
  const color = look.highlightColor;
  const style = {
    "--mark": color,
    "--mark-ink": inkFor(color),
    "--thick": `${look.highlightThickness}px`,
    "--t": String(1 / look.animationSpeed),
  } as CSSProperties;
  const holes = marks.flatMap((m) => (m.type === "highlight" ? [m] : []));
  const root = useRef<HTMLDivElement>(null);

  // Labels placed by different marks can land on each other; each one that
  // would cover an earlier label or writing moves down clear of it.
  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;
    const placed: DOMRect[] = [];
    for (const node of el.querySelectorAll<HTMLElement>(".ann__tag, .ann__bubble, .ann__write, .ann__picture")) {
      node.style.marginTop = "";
      let r = node.getBoundingClientRect();
      if (r.width === 0) continue;
      if (!node.matches(".ann__write, .ann__picture")) {
        let shift = 0;
        for (let hit = overlap(r, placed); hit; hit = overlap(r, placed)) {
          shift += hit.bottom - r.top + 4;
          node.style.marginTop = `${shift}px`;
          r = node.getBoundingClientRect();
        }
      }
      placed.push(r);
    }
  }, [marks]);
  const classes = ["ann", look.glow && "ann--glow", look.reduceMotion && "ann--still", `ann--${look.labelStyle}`].filter(Boolean).join(" ");

  return (
    <div ref={root} className={classes} style={style}>
      <svg className="ann__svg" width={width} height={height} viewBox={`0 0 ${width} ${height}`} aria-hidden="true">
        {look.dim > 0 && holes.length > 0 && (
          <path className="ann__dim" d={dimPath(width, height, holes)} fillRule="evenodd" style={{ fill: `rgba(8, 10, 14, ${look.dim})` }} />
        )}
        {marks.map((m, i) => {
          if (m.type === "highlight") {
            const x = m.x - HIGHLIGHT_PAD;
            const y = m.y - HIGHLIGHT_PAD;
            const w = m.width + HIGHLIGHT_PAD * 2;
            const h = m.height + HIGHLIGHT_PAD * 2;
            const r = Math.min(10, w / 2, h / 2);
            return (
              <g key={i} className="ann__stroke" style={timed(timing, i)}>
                <rect className="ann__pulse" x={x} y={y} width={w} height={h} rx={r} />
                <rect className="ann__box" x={x} y={y} width={w} height={h} rx={r} pathLength={1} />
              </g>
            );
          }
          if (m.type === "arrow") {
            const a = arrow({ x: m.fromX, y: m.fromY }, { x: m.toX, y: m.toY }, look.arrowStyle === "curved", look.highlightThickness);
            return (
              <g key={i} className="ann__stroke" style={timed(timing, i)}>
                <path className="ann__shaft" d={a.shaft} pathLength={1} />
                <polygon className="ann__head" points={a.head.map((p) => `${p.x},${p.y}`).join(" ")} />
              </g>
            );
          }
          if (m.type === "line") {
            const d = linePath(
              m.points.map(([x, y]) => ({ x, y })),
              m.closed,
              m.curved,
            );
            return (
              <g key={i} className="ann__stroke" style={timed(timing, i)}>
                <path className="ann__line" d={d} pathLength={1} />
              </g>
            );
          }
          if (m.type === "text" || m.type === "image") return null;
          return (
            <g key={i} className="ann__appear" style={timed(timing, i)}>
              <circle className="ann__ring" cx={m.x} cy={m.y} r={14} />
              <circle className="ann__dot" cx={m.x} cy={m.y} r={4.5} />
            </g>
          );
        })}
      </svg>

      {marks.map((m, i) => (
        <div key={i} style={timed(timing, i)}>
          <MarkLabels mark={m} debug={look.showCoordinates} />
        </div>
      ))}
    </div>
  );
}

/** The first placed box `r` overlaps, if any. */
function overlap(r: DOMRect, placed: DOMRect[]): DOMRect | undefined {
  return placed.find((p) => r.left < p.right && r.right > p.left && r.top < p.bottom && r.bottom > p.top);
}

function MarkLabels({ mark: m, debug }: { mark: Mark; debug: boolean }) {
  const raw = debug && <span className="ann__raw">{m.raw}</span>;
  if (m.type === "highlight") {
    const above = labelAbove(m.y);
    const top = above ? m.y - HIGHLIGHT_PAD - 8 : m.y + m.height + HIGHLIGHT_PAD + 8;
    return (
      <div className={`ann__tag ${above ? "ann__tag--above" : ""}`} style={{ left: m.x - HIGHLIGHT_PAD, top }}>
        {m.label && <span className="ann__label">{m.label}</span>}
        {raw}
      </div>
    );
  }
  if (m.type === "arrow") {
    // The label sits at the tail, pushed away from where the arrow heads.
    const left = m.toX >= m.fromX;
    return (
      <div className={`ann__tag ann__tag--mid ${left ? "ann__tag--end" : ""}`} style={{ left: m.fromX + (left ? -10 : 10), top: m.fromY }}>
        {m.label && <span className="ann__label">{m.label}</span>}
        {raw}
      </div>
    );
  }
  if (m.type === "text") {
    return (
      <div className="ann__write" style={{ left: m.x, top: m.y, fontSize: m.size }}>
        <span>{m.text}</span>
        {raw}
      </div>
    );
  }
  if (m.type === "image") {
    return (
      <figure className="ann__picture" style={{ left: m.x, top: m.y, width: m.width }}>
        <img src={m.src} alt={m.caption ?? ""} />
        {m.caption && <figcaption>{m.caption}</figcaption>}
        {raw}
      </figure>
    );
  }
  if (m.type === "line") {
    if (!m.label && !raw) return null;
    const at = lineLabelAt(
      m.points.map(([x, y]) => ({ x, y })),
      m.closed,
      m.curved,
    );
    // Inside a shape; just above the middle of an open line.
    return (
      <div className={`ann__tag ${m.closed ? "ann__tag--centre" : "ann__tag--over"}`} style={{ left: at.x, top: at.y }}>
        {m.label && <span className="ann__label">{m.label}</span>}
        {raw}
      </div>
    );
  }
  const below = bubbleBelow(m.y);
  return (
    <div className={`ann__bubble ${below ? "ann__bubble--below" : ""}`} style={{ left: m.x, top: m.y }}>
      <span className="ann__label ann__label--bubble">{m.label ?? "Here"}</span>
      {raw}
    </div>
  );
}
