import type { BuddyStyle } from "../bindings/BuddyStyle";
import "./buddy.css";

export type BuddyState = "idle" | "listening" | "thinking" | "speaking";

type Props = {
  style: BuddyStyle;
  size: number;
  state?: BuddyState;
  animate?: boolean;
  /** Running agents; hidden when 0 or null. */
  badge?: number | null;
  /** Questions waiting behind the one being answered. */
  queued?: number;
  customSrc?: string | null;
};

// Pip's top-left corner is a point aimed back at the cursor.
const PIP = "M6 6 L26 13.5 A22 22 0 1 1 13.5 26 Z";
// A plump four-point sparkle: tips are sharp, the middle is wide enough for a face.
const SPARK = "M34 4 Q46 20 62 32 Q46 44 34 60 Q22 44 6 32 Q22 20 34 4 Z";

export function Buddy({ style, size, state = "idle", animate = true, badge, queued = 0, customSrc }: Props) {
  const cls = `buddy buddy--${state} buddy--${style}${animate ? " buddy--animate" : ""}`;
  return (
    <div className={cls} style={{ width: size, height: size }} aria-hidden="true">
      {style === "custom" && customSrc ? (
        <img className="buddy__img" src={customSrc} alt="" draggable={false} />
      ) : (
        <svg viewBox="0 0 64 64" width={size} height={size}>
          <circle className="buddy__ring" cx="34" cy="34" r="24" />
          {style === "dot" ? (
            <g className="buddy__body">
              <circle cx="34" cy="34" r="19" className="buddy__dot-ring" />
              <circle cx="34" cy="34" r="8" className="buddy__fill" />
            </g>
          ) : (
            <g className="buddy__body">
              <path d={style === "spark" ? SPARK : PIP} className="buddy__fill" />
              <g className="buddy__eyes">
                <ellipse cx={29} cy={style === "spark" ? 31 : 33} rx="2.9" ry="4" />
                <ellipse cx={style === "spark" ? 39 : 40} cy={style === "spark" ? 31 : 33} rx="2.9" ry="4" />
              </g>
              <rect className="buddy__mouth" x="31" y="42" width="6" height="3" rx="1.5" />
            </g>
          )}
          <g className="buddy__dots">
            <circle cx="52" cy="14" r="2.6" />
            <circle cx="58" cy="8" r="2" />
            <circle cx="62" cy="3" r="1.4" />
          </g>
        </svg>
      )}
      {badge ? <span className="buddy__badge">{badge}</span> : null}
      {queued > 0 ? <span className="buddy__badge buddy__badge--queue">{queued}</span> : null}
    </div>
  );
}
