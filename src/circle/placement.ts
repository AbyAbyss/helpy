// Where Circle to explain puts its card and its diagram labels. Pure, so the
// no-overlap rules can be tested.

export type Rect = { x: number; y: number; w: number; h: number };
export type Size = { w: number; h: number };
export type Pt = { x: number; y: number };

const GAP = 16;
/** Space kept between labels, and between labels and everything else. */
const PAD = 6;
const EDGE = 8;

export function overlaps(a: Rect, b: Rect, pad = 0): boolean {
  return a.x < b.x + b.w + pad && b.x < a.x + a.w + pad && a.y < b.y + b.h + pad && b.y < a.y + a.h + pad;
}

function inside(r: Rect, bounds: Size): boolean {
  return r.x >= EDGE && r.y >= EDGE && r.x + r.w <= bounds.w - EDGE && r.y + r.h <= bounds.h - EDGE;
}

/** The card goes beside the selection: right, left, below, above, in that order. */
export function placeCard(sel: Rect, card: Size, bounds: Size): Rect {
  const clampY = (y: number) => Math.min(Math.max(y, EDGE), bounds.h - card.h - EDGE);
  const clampX = (x: number) => Math.min(Math.max(x, EDGE), bounds.w - card.w - EDGE);
  const tries: Rect[] = [
    { x: sel.x + sel.w + GAP, y: clampY(sel.y), ...card },
    { x: sel.x - GAP - card.w, y: clampY(sel.y), ...card },
    { x: clampX(sel.x), y: sel.y + sel.h + GAP, ...card },
    { x: clampX(sel.x), y: sel.y - GAP - card.h, ...card },
  ];
  // A selection filling the screen: the card sits over its bottom-right corner.
  return (
    tries.find((r) => inside(r, bounds)) ?? {
      x: Math.max(EDGE, bounds.w - card.w - EDGE * 3),
      y: Math.max(EDGE, bounds.h - card.h - EDGE * 3),
      ...card,
    }
  );
}

/** How far along a ray from the centre of `r` it leaves the rectangle. */
function exitDistance(r: Rect, dx: number, dy: number): number {
  const tx = dx === 0 ? Infinity : r.w / 2 / Math.abs(dx);
  const ty = dy === 0 ? Infinity : r.h / 2 / Math.abs(dy);
  return Math.min(tx, ty);
}

/**
 * Places one label per anchor around the selection, like a textbook diagram:
 * each label goes outward in its anchor's direction, and slides around the
 * selection until it overlaps nothing. Labels that can't fit are null.
 */
export function placeLabels(sel: Rect, anchors: Pt[], sizes: Size[], bounds: Size, obstacles: Rect[] = []): (Rect | null)[] {
  const cx = sel.x + sel.w / 2;
  const cy = sel.y + sel.h / 2;
  const placed: (Rect | null)[] = anchors.map(() => null);
  const taken: Rect[] = [];
  // Around the clock from the top, so neighbours on the diagram stay neighbours.
  const angle = (p: Pt) => (p.x === cx && p.y === cy ? -Math.PI / 2 : Math.atan2(p.y - cy, p.x - cx));
  const order = anchors.map((p, i) => ({ i, a: angle(p) })).sort((p, q) => p.a - q.a);

  for (const { i, a } of order) {
    const size = sizes[i];
    search: for (const ring of [0, 36, 72, 120]) {
      for (let k = 0; k <= 15; k++) {
        // 0, +12°, -12°, +24°, -24° … up to 180° either way.
        const turn = (k % 2 ? 1 : -1) * Math.ceil(k / 2) * (Math.PI / 15);
        const t = a + turn;
        const dx = Math.cos(t);
        const dy = Math.sin(t);
        const dist = exitDistance(sel, dx, dy) + GAP + ring;
        const px = cx + dx * dist;
        const py = cy + dy * dist;
        // Grow away from the selection from the point on the ring.
        const x = dx > 0.35 ? px : dx < -0.35 ? px - size.w : px - size.w / 2;
        const y = dy > 0.35 ? py : dy < -0.35 ? py - size.h : py - size.h / 2;
        const r = { x, y, w: size.w, h: size.h };
        if (!inside(r, bounds) || overlaps(r, sel, PAD)) continue;
        if (taken.some((o) => overlaps(r, o, PAD)) || obstacles.some((o) => overlaps(r, o, PAD))) continue;
        placed[i] = r;
        taken.push(r);
        break search;
      }
    }
  }
  return placed;
}

/** Where a leader line meets its label: the nearest point on the label's box. */
export function nearestOnRect(p: Pt, r: Rect): Pt {
  return { x: Math.min(Math.max(p.x, r.x), r.x + r.w), y: Math.min(Math.max(p.y, r.y), r.y + r.h) };
}

/** The bounding box of drawn points. */
export function boundsOf(points: Pt[]): Rect {
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
}
