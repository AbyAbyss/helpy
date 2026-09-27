// The buddy's moves that aren't following the cursor: touring a step's
// marks while they draw, and the copy that flies off to become an agent.
// Pure so it can be tested; the overlay plays it frame by frame.

import type { Mark } from "../bindings/Mark";
import { arrow, HIGHLIGHT_PAD, pointAlong, polylineLength, sampleLine, textWidth, type Pt } from "../guide/geometry";

/** One mark on the tour, in ms from the tour's start. */
export type Leg = {
  /** The buddy leaves for the mark. */
  fly: number;
  /** It arrives and the mark starts drawing. */
  draw: number;
  /** How long the mark takes to draw (0 for a pointer). */
  dur: number;
  /** The path the buddy traces while it draws. */
  trace: Pt[];
};

export type Tour = { from: Pt; legs: Leg[]; end: number };

/** Flying between marks and drawing them, CSS pixels per ms. */
const FLY_SPEED = 1.8;
const DRAW_SPEED = 0.75;
/** A breath after each mark, and a longer look at each pointer, ms. */
const PAUSE = 140;
const POINT_PAUSE = 320;

/** The path a mark draws along, in the order the stroke appears. */
export function traceOf(m: Mark, curvedArrows: boolean): Pt[] {
  switch (m.type) {
    case "highlight": {
      const x0 = m.x - HIGHLIGHT_PAD;
      const y0 = m.y - HIGHLIGHT_PAD;
      const x1 = m.x + m.width + HIGHLIGHT_PAD;
      const y1 = m.y + m.height + HIGHLIGHT_PAD;
      // An SVG rect's stroke starts at its top-left and runs clockwise.
      return [
        { x: x0, y: y0 },
        { x: x1, y: y0 },
        { x: x1, y: y1 },
        { x: x0, y: y1 },
        { x: x0, y: y0 },
      ];
    }
    case "arrow": {
      const from = { x: m.fromX, y: m.fromY };
      const to = { x: m.toX, y: m.toY };
      const c = arrow(from, to, curvedArrows, 3).control;
      if (!c) return [from, to];
      return Array.from({ length: 17 }, (_, k) => {
        const t = k / 16;
        const u = 1 - t;
        return { x: u * u * from.x + 2 * u * t * c.x + t * t * to.x, y: u * u * from.y + 2 * u * t * c.y + t * t * to.y };
      });
    }
    case "line":
      return sampleLine(
        m.points.map(([x, y]) => ({ x, y })),
        m.closed,
        m.curved,
      );
    case "point":
      return [{ x: m.x, y: m.y }];
    case "image":
      // Across the top of the picture as it opens.
      return [
        { x: m.x, y: m.y },
        { x: m.x + m.width, y: m.y },
      ];
    case "text": {
      // Along the writing's baseline, left to right, as it's written in.
      const half = textWidth(m.text, m.size) / 2;
      const y = m.y + m.size * 0.35;
      return [
        { x: m.x - half, y },
        { x: m.x + half, y },
      ];
    }
  }
}

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/**
 * Plans the tour: from `from`, visit each mark in order, drawing it on
 * arrival. `scale` stretches every duration (1 / animation speed).
 */
export function planTour(marks: Mark[], from: Pt, scale: number, curvedArrows: boolean): Tour {
  const legs: Leg[] = [];
  let at = from;
  let t = 0;
  for (const m of marks) {
    const trace = traceOf(m, curvedArrows);
    const start = trace[0];
    const fly = clamp(Math.hypot(start.x - at.x, start.y - at.y) / FLY_SPEED, 160, 620) * scale;
    const dur = m.type === "point" ? 0 : clamp(polylineLength(trace) / DRAW_SPEED, 380, 1500) * scale;
    legs.push({ fly: t, draw: t + fly, dur, trace });
    t += fly + dur + (m.type === "point" ? POINT_PAUSE : PAUSE) * scale;
    at = trace[trace.length - 1];
  }
  return { from, legs, end: t };
}

/** CSS `ease-in-out` (cubic-bezier(0.42, 0, 0.58, 1)), matching the stroke's draw-in. */
export function easeInOut(x: number): number {
  if (x <= 0) return 0;
  if (x >= 1) return 1;
  const [x1, x2] = [0.42, 0.58];
  const bez = (t: number, a: number, b: number) => 3 * (1 - t) * (1 - t) * t * a + 3 * (1 - t) * t * t * b + t * t * t;
  // Solve bez(t) = x for t by bisection; the curve is monotonic.
  let lo = 0;
  let hi = 1;
  for (let i = 0; i < 24; i++) {
    const mid = (lo + hi) / 2;
    if (bez(mid, x1, x2) < x) lo = mid;
    else hi = mid;
  }
  return bez((lo + hi) / 2, 0, 1);
}

/** Where the buddy's tip is `ms` into the tour. */
export function tourAt(tour: Tour, ms: number): Pt {
  let prev = tour.from;
  for (const leg of tour.legs) {
    const start = leg.trace[0];
    if (ms < leg.fly) return prev;
    if (ms < leg.draw) {
      const e = easeInOut((ms - leg.fly) / (leg.draw - leg.fly));
      return { x: prev.x + (start.x - prev.x) * e, y: prev.y + (start.y - prev.y) * e };
    }
    if (ms < leg.draw + leg.dur) return pointAlong(leg.trace, easeInOut((ms - leg.draw) / leg.dur));
    prev = leg.trace[leg.trace.length - 1];
  }
  return prev;
}

/** One keyframe of the hand-off flight: where the copy's centre is. */
export type Frame = { offset: number; x: number; y: number; angle: number; scale: number; opacity: number };

/** The buddy's pointed tip aims up and to the left (225°) when unrotated. */
const TIP_ANGLE = 225;

/**
 * The copy that peels off the buddy and flies to the dock: a short hop
 * sideways out of the buddy, then an arc over to the chip, nose first,
 * shrinking to chip size and fading as the chip drops in.
 */
export function handoffFrames(from: Pt, to: Pt, steps = 16): Frame[] {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const dist = Math.hypot(dx, dy) || 1;
  // Peel off a little way from the buddy, away from where it's heading.
  const peel = { x: from.x - (dx / dist) * 18, y: from.y - 26 };
  // The arc bows upward, by more on a longer trip.
  const lift = clamp(dist * 0.28, 60, 260);
  const control = { x: (peel.x + to.x) / 2, y: Math.min(peel.y, to.y) - lift };
  const at = (t: number) => {
    const u = 1 - t;
    return { x: u * u * peel.x + 2 * u * t * control.x + t * t * to.x, y: u * u * peel.y + 2 * u * t * control.y + t * t * to.y };
  };
  const heading = (t: number) => {
    const a = at(Math.max(0, t - 0.02));
    const b = at(Math.min(1, t + 0.02));
    return (Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI - TIP_ANGLE;
  };
  const frames: Frame[] = [
    { offset: 0, x: from.x, y: from.y, angle: 0, scale: 1, opacity: 1 },
    { offset: 0.14, x: peel.x, y: peel.y, angle: heading(0), scale: 1.18, opacity: 1 },
  ];
  for (let i = 1; i <= steps; i++) {
    const t = i / steps;
    const p = at(t);
    frames.push({
      offset: 0.14 + 0.86 * t,
      x: p.x,
      y: p.y,
      angle: heading(t),
      scale: 1.18 - 0.6 * t,
      opacity: t < 0.82 ? 1 : 1 - (t - 0.82) / 0.18,
    });
  }
  // Keep turning the short way round, so it never spins a full circle.
  for (let i = 1; i < frames.length; i++) {
    while (frames[i].angle - frames[i - 1].angle > 180) frames[i].angle -= 360;
    while (frames[i].angle - frames[i - 1].angle < -180) frames[i].angle += 360;
  }
  return frames;
}
