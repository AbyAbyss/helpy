// Pure geometry for guidance marks, kept apart from React so it can be tested.

export type Pt = { x: number; y: number };

/** Space left around a highlighted control, CSS pixels. */
export const HIGHLIGHT_PAD = 6;

/** Readable text colour (near-black or white) on a "#rrggbb" background. */
export function inkFor(hex: string): string {
  const n = parseInt(hex.slice(1), 16);
  if (Number.isNaN(n) || hex.length !== 7) return "#ffffff";
  const lin = (c: number) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  const l = 0.2126 * lin((n >> 16) & 255) + 0.7152 * lin((n >> 8) & 255) + 0.0722 * lin(n & 255);
  // White, as on Helpy's own accent buttons, unless it would fall below 3:1
  // (enough for the bold label text); then near-black.
  return 1.05 / (l + 0.05) >= 3 ? "#ffffff" : "#12151c";
}

export type ArrowShape = {
  /** SVG path for the shaft, ending where the head starts. */
  shaft: string;
  /** Three points of the head; the first is the tip. */
  head: [Pt, Pt, Pt];
  length: number;
  /** The curve's control point, or null for a straight arrow. */
  control: Pt | null;
};

/**
 * An arrow from `from` to `to`. Curved arrows bow to one side by a fifth of
 * their length, always upward so they arc over content instead of under it.
 */
export function arrow(from: Pt, to: Pt, curved: boolean, width: number): ArrowShape {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const length = Math.hypot(dx, dy) || 1;
  let control: Pt | null = null;
  if (curved && length > 40) {
    const mid = { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 };
    // Unit normal, flipped so it points up the screen.
    let nx = -dy / length;
    let ny = dx / length;
    if (ny > 0 || (ny === 0 && nx > 0)) {
      nx = -nx;
      ny = -ny;
    }
    const bow = length * 0.2;
    control = { x: mid.x + nx * bow, y: mid.y + ny * bow };
  }
  // The head points along the curve's direction at its end.
  const back = control ?? from;
  const bl = Math.hypot(to.x - back.x, to.y - back.y) || 1;
  const ux = (to.x - back.x) / bl;
  const uy = (to.y - back.y) / bl;
  const headLen = Math.min(10 + width * 2.5, length * 0.45);
  const headHalf = headLen * 0.55;
  const base = { x: to.x - ux * headLen, y: to.y - uy * headLen };
  const head: [Pt, Pt, Pt] = [
    to,
    { x: base.x - uy * headHalf, y: base.y + ux * headHalf },
    { x: base.x + uy * headHalf, y: base.y - ux * headHalf },
  ];
  // Stop the shaft inside the head so the round cap doesn't poke out.
  const end = { x: to.x - ux * headLen * 0.8, y: to.y - uy * headLen * 0.8 };
  const f = (p: Pt) => `${p.x.toFixed(1)} ${p.y.toFixed(1)}`;
  const shaft = control ? `M ${f(from)} Q ${f(control)} ${f(end)}` : `M ${f(from)} L ${f(end)}`;
  return { shaft, head, length, control };
}

/** The cubic Bézier pieces of a line: straight, or a smooth curve through every point. */
function pieces(points: Pt[], closed: boolean, curved: boolean): [Pt, Pt, Pt, Pt][] {
  const n = points.length;
  const count = closed ? n : n - 1;
  const at = (i: number) => (closed ? points[(i + n) % n] : points[Math.max(0, Math.min(n - 1, i))]);
  const out: [Pt, Pt, Pt, Pt][] = [];
  for (let i = 0; i < count; i++) {
    const p1 = at(i);
    const p2 = at(i + 1);
    if (!curved) {
      out.push([p1, p1, p2, p2]);
      continue;
    }
    // Catmull-Rom through the points, as Bézier control points.
    const p0 = at(i - 1);
    const p3 = at(i + 2);
    out.push([p1, { x: p1.x + (p2.x - p0.x) / 6, y: p1.y + (p2.y - p0.y) / 6 }, { x: p2.x - (p3.x - p1.x) / 6, y: p2.y - (p3.y - p1.y) / 6 }, p2]);
  }
  return out;
}

/** SVG path for a line mark. */
export function linePath(points: Pt[], closed: boolean, curved: boolean): string {
  if (points.length === 0) return "";
  const f = (p: Pt) => `${p.x.toFixed(1)} ${p.y.toFixed(1)}`;
  let d = `M ${f(points[0])}`;
  for (const [, c1, c2, p] of pieces(points, closed, curved)) d += curved ? ` C ${f(c1)} ${f(c2)} ${f(p)}` : ` L ${f(p)}`;
  return closed ? `${d} Z` : d;
}

/** Points along a line mark, in drawing order, close enough together to trace. */
export function sampleLine(points: Pt[], closed: boolean, curved: boolean, steps = 12): Pt[] {
  if (points.length === 0) return [];
  const out = [points[0]];
  for (const [a, b, c, d] of pieces(points, closed, curved)) {
    if (!curved) {
      out.push(d);
      continue;
    }
    for (let k = 1; k <= steps; k++) {
      const t = k / steps;
      const u = 1 - t;
      out.push({
        x: u * u * u * a.x + 3 * u * u * t * b.x + 3 * u * t * t * c.x + t * t * t * d.x,
        y: u * u * u * a.y + 3 * u * u * t * b.y + 3 * u * t * t * c.y + t * t * t * d.y,
      });
    }
  }
  return out;
}

/** Where a line's label goes: the middle of an open line, the centre of a shape. */
export function lineLabelAt(points: Pt[], closed: boolean, curved: boolean): Pt {
  if (closed) {
    const sum = points.reduce((a, p) => ({ x: a.x + p.x, y: a.y + p.y }), { x: 0, y: 0 });
    return { x: sum.x / points.length, y: sum.y / points.length };
  }
  return pointAlong(sampleLine(points, closed, curved), 0.5);
}

/** Total length of a polyline. */
export function polylineLength(pts: Pt[]): number {
  let len = 0;
  for (let i = 1; i < pts.length; i++) len += Math.hypot(pts[i].x - pts[i - 1].x, pts[i].y - pts[i - 1].y);
  return len;
}

/** The point a share `t` (0 to 1) of the way along a polyline. */
export function pointAlong(pts: Pt[], t: number): Pt {
  if (pts.length < 2) return pts[0] ?? { x: 0, y: 0 };
  let left = Math.max(0, Math.min(1, t)) * polylineLength(pts);
  for (let i = 1; i < pts.length; i++) {
    const a = pts[i - 1];
    const b = pts[i];
    const seg = Math.hypot(b.x - a.x, b.y - a.y);
    if (left <= seg && seg > 0) return { x: a.x + ((b.x - a.x) * left) / seg, y: a.y + ((b.y - a.y) * left) / seg };
    left -= seg;
  }
  return pts[pts.length - 1];
}

/** Whether a label for a box at `top` fits above it, or must go below. */
export function labelAbove(top: number, labelHeight = 30): boolean {
  return top - HIGHLIGHT_PAD - labelHeight - 6 >= 0;
}

/** Where to put a pointer's bubble so its tail touches the point. */
export function bubbleBelow(y: number, bubbleHeight = 44): boolean {
  return y - bubbleHeight - 10 < 0;
}

/**
 * The dimmed area: the whole screen minus a rounded hole per highlight,
 * drawn with the even-odd rule.
 */
export function dimPath(width: number, height: number, holes: { x: number; y: number; width: number; height: number }[], radius = 10): string {
  let d = `M0 0H${width}V${height}H0Z`;
  for (const h of holes) {
    const x = h.x - HIGHLIGHT_PAD;
    const y = h.y - HIGHLIGHT_PAD;
    const w = h.width + HIGHLIGHT_PAD * 2;
    const hh = h.height + HIGHLIGHT_PAD * 2;
    const r = Math.min(radius, w / 2, hh / 2);
    d += `M${x + r} ${y}H${x + w - r}A${r} ${r} 0 0 1 ${x + w} ${y + r}V${y + hh - r}A${r} ${r} 0 0 1 ${x + w - r} ${y + hh}H${x + r}A${r} ${r} 0 0 1 ${x} ${y + hh - r}V${y + r}A${r} ${r} 0 0 1 ${x + r} ${y}Z`;
  }
  return d;
}
