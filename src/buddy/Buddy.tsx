import type { BuddyStyle } from "../bindings/BuddyStyle";
import type { BuddyActivity } from "../bindings/BuddyActivity";
import "./buddy.css";

export interface BuddyProps {
  style: BuddyStyle;
  size: number;
  activity: BuddyActivity;
  animate: boolean;
  agents?: number;
  showBadge?: boolean;
  attention?: boolean;
  customSrc?: string | null;
  /** Mirror horizontally when the buddy flips to the left of the cursor. */
  flipped?: boolean;
}

/** The body's tip points up-left, toward the cursor hotspot. */
const PIP_BODY =
  "M9 9 C 17 10.5 24 12.5 28.5 15.5 C 31 14.6 33.5 14 36 14 C 48.2 14 58 23.8 58 36 C 58 48.2 48.2 58 36 58 C 23.8 58 14 48.2 14 36 C 14 33 14.6 30.2 15.6 27.6 C 12.6 22.8 10.4 16.4 9 9 Z";

function Eyes({ fill, cx = [30, 42.5], cy = 36 }: { fill: string; cx?: [number, number]; cy?: number }) {
  return (
    <g className="bd-eyes">
      <ellipse cx={cx[0]} cy={cy} rx="3.2" ry="4.4" fill={fill} />
      <ellipse cx={cx[1]} cy={cy} rx="3.2" ry="4.4" fill={fill} />
    </g>
  );
}

function Mouth({ fill, cx = 36.2, cy = 46 }: { fill: string; cx?: number; cy?: number }) {
  return <ellipse className="bd-mouth" cx={cx} cy={cy} rx="3.6" ry="2.2" fill={fill} />;
}

function ThinkingDots({ color }: { color: string }) {
  return (
    <g className="bd-think">
      <circle cx="50" cy="12" r="2.6" fill={color} />
      <circle cx="56.5" cy="17" r="2.6" fill={color} />
      <circle cx="59" cy="25" r="2.6" fill={color} />
    </g>
  );
}

function Art({ style }: { style: Exclude<BuddyStyle, "custom"> }) {
  switch (style) {
    case "pip":
      return (
        <>
          <path d={PIP_BODY} fill="var(--pip)" />
          <path d="M22 26 C 25 20 31 17.5 36 17.5" fill="none" stroke="#fff" strokeOpacity=".45" strokeWidth="3" strokeLinecap="round" />
          <Eyes fill="var(--pip-ink)" />
          <Mouth fill="var(--pip-ink)" />
          <ThinkingDots color="var(--pip)" />
        </>
      );
    case "orbit":
      return (
        <>
          <circle cx="34" cy="34" r="17" fill="#fff" fillOpacity=".92" />
          <circle cx="34" cy="34" r="17" fill="none" stroke="#1f2a27" strokeWidth="3.5" />
          <circle cx="34" cy="34" r="6.5" fill="#1f2a27" className="bd-core" />
          <g className="bd-orbit">
            <circle cx="34" cy="10" r="5" fill="var(--pip)" stroke="#fff" strokeWidth="2" />
          </g>
          <ThinkingDots color="#1f2a27" />
        </>
      );
    case "spark":
      return (
        <>
          <path
            className="bd-spark"
            d="M34 6 C 36 22 40 28 58 32 C 40 36 36 42 34 60 C 32 42 28 36 10 32 C 28 28 32 22 34 6 Z"
            fill="#ffb020"
            stroke="#fff"
            strokeWidth="2.5"
            strokeLinejoin="round"
          />
          <circle cx="34" cy="33" r="4.5" fill="#fff" fillOpacity=".85" />
          <ThinkingDots color="#ffb020" />
        </>
      );
    case "pebble":
      return (
        <>
          <rect x="12" y="14" width="44" height="42" rx="15" fill="#1f2a27" stroke="#fff" strokeWidth="2.5" />
          <path d="M8 8 L 20 13 L 13 20 Z" fill="#1f2a27" stroke="#fff" strokeWidth="2" strokeLinejoin="round" />
          <Eyes fill="#ffffff" cx={[27.5, 40.5]} cy={34} />
          <Mouth fill="#ffffff" cx={34} cy={45} />
          <ThinkingDots color="#1f2a27" />
        </>
      );
  }
}

export function Buddy({ style, size, activity, animate, agents = 0, showBadge = true, attention = false, customSrc, flipped }: BuddyProps) {
  const useCustom = style === "custom" && customSrc;
  const artStyle: Exclude<BuddyStyle, "custom"> = style === "custom" ? "pip" : style;
  return (
    <div
      className="bd"
      data-activity={activity}
      data-animate={animate ? "on" : "off"}
      data-flipped={flipped ? "yes" : "no"}
      style={{ width: size, height: size }}
      aria-hidden="true"
    >
      <span className="bd-ring" />
      <div className="bd-body">
        {useCustom ? (
          <img src={customSrc!} alt="" draggable={false} />
        ) : (
          <svg viewBox="0 0 64 64" width={size} height={size}>
            <Art style={artStyle} />
          </svg>
        )}
      </div>
      {showBadge && agents > 0 && <span className="bd-badge">{agents > 9 ? "9+" : agents}</span>}
      {attention && <span className="bd-attn" />}
    </div>
  );
}
