import type { CSSProperties } from "react";
import type { Guidance } from "../bindings/Guidance";
import type { Mark } from "../bindings/Mark";
import { arrow, bubbleBelow, dimPath, HIGHLIGHT_PAD, inkFor, labelAbove } from "./geometry";
import "./annotations.css";

type Props = {
  marks: Mark[];
  /** The drawing area in CSS pixels (the monitor, or the settings preview). */
  width: number;
  height: number;
  look: Guidance;
};

/**
 * Highlights, pointers and arrows for one guidance step. Purely visual: it
 * never takes pointer events, so the app underneath stays usable.
 */
export function Annotations({ marks, width, height, look }: Props) {
  const color = look.highlightColor;
  const style = {
    "--mark": color,
    "--mark-ink": inkFor(color),
    "--thick": `${look.highlightThickness}px`,
    "--t": String(1 / look.animationSpeed),
  } as CSSProperties;
  const holes = marks.flatMap((m) => (m.type === "highlight" ? [m] : []));
  const classes = ["ann", look.glow && "ann--glow", look.reduceMotion && "ann--still", `ann--${look.labelStyle}`].filter(Boolean).join(" ");

  return (
    <div className={classes} style={style}>
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
              <g key={i} className="ann__stroke">
                <rect className="ann__pulse" x={x} y={y} width={w} height={h} rx={r} />
                <rect className="ann__box" x={x} y={y} width={w} height={h} rx={r} pathLength={1} />
              </g>
            );
          }
          if (m.type === "arrow") {
            const a = arrow({ x: m.fromX, y: m.fromY }, { x: m.toX, y: m.toY }, look.arrowStyle === "curved", look.highlightThickness);
            return (
              <g key={i} className="ann__stroke">
                <path className="ann__shaft" d={a.shaft} pathLength={1} />
                <polygon className="ann__head" points={a.head.map((p) => `${p.x},${p.y}`).join(" ")} />
              </g>
            );
          }
          return (
            <g key={i}>
              <circle className="ann__ring" cx={m.x} cy={m.y} r={14} />
              <circle className="ann__dot" cx={m.x} cy={m.y} r={4.5} />
            </g>
          );
        })}
      </svg>

      {marks.map((m, i) => (
        <MarkLabels key={i} mark={m} debug={look.showCoordinates} />
      ))}
    </div>
  );
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
  const below = bubbleBelow(m.y);
  return (
    <div className={`ann__bubble ${below ? "ann__bubble--below" : ""}`} style={{ left: m.x, top: m.y }}>
      <span className="ann__label ann__label--bubble">{m.label ?? "Here"}</span>
      {raw}
    </div>
  );
}
