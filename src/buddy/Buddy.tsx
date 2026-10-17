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
  /** A baby of the buddy: rounder, bigger eyes, rosy cheeks. */
  baby?: boolean;
};

// Pip's top-left corner is a point aimed back at the cursor.
const PIP = "M6 6 L26 13.5 A22 22 0 1 1 13.5 26 Z";
// A plump four-point sparkle: tips are sharp, the middle is wide enough for a face.
const SPARK = "M34 4 Q46 20 62 32 Q46 44 34 60 Q22 44 6 32 Q22 20 34 4 Z";
// Babies: a rounder pip with a stubby point, and a plumper sparkle.
const BABY_PIP = "M11 11 L27 17.5 A20.5 20.5 0 1 1 17.5 27 Z";
const BABY_SPARK = "M34 7 Q44 24 59 32 Q44 40 34 57 Q24 40 9 32 Q24 24 34 7 Z";

export function Buddy({ style, size, state = "idle", animate = true, badge, queued = 0, customSrc, baby = false }: Props) {
  const cls = `buddy buddy--${state} buddy--${style}${animate ? " buddy--animate" : ""}${baby ? " buddy--baby" : ""}`;
  const spark = style === "spark";
  // Babies' eyes sit lower and wider apart, over the cheeks.
  const eyeY = baby ? 36 : spark ? 31 : 33;
  const eyeX: [number, number] = baby ? [28, 41] : spark ? [29, 39] : [29, 40];
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
              <path d={baby ? (spark ? BABY_SPARK : BABY_PIP) : spark ? SPARK : PIP} className="buddy__fill" />
              {baby && (
                <g className="buddy__cheeks">
                  <ellipse cx={24} cy={42} rx="3.4" ry="2.1" />
                  <ellipse cx={45} cy={42} rx="3.4" ry="2.1" />
                </g>
              )}
              <g className="buddy__eyes">
                <ellipse cx={eyeX[0]} cy={eyeY} rx={baby ? 3.6 : 2.9} ry={baby ? 4.8 : 4} />
                <ellipse cx={eyeX[1]} cy={eyeY} rx={baby ? 3.6 : 2.9} ry={baby ? 4.8 : 4} />
                {baby && <circle className="buddy__glint" cx={eyeX[0] + 1.2} cy={eyeY - 1.8} r="1.1" />}
                {baby && <circle className="buddy__glint" cx={eyeX[1] + 1.2} cy={eyeY - 1.8} r="1.1" />}
              </g>
              {baby ? (
                <path className="buddy__smile" d="M31 45 Q34.5 48 38 45" />
              ) : (
                <rect className="buddy__mouth" x="31" y="42" width="6" height="3" rx="1.5" />
              )}
              {baby && <rect className="buddy__mouth" x="31.5" y="44" width="6" height="3" rx="1.5" />}
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
