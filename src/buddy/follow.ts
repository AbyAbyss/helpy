// Cursor-follow physics shared by the overlay and the settings preview.
//
// Exponential smoothing that is independent of frame rate: at smoothness 0 the buddy
// lands on the target every frame (no perceptible lag), and higher values glide.

export interface Vec {
  x: number;
  y: number;
}

/** Converts the 0..1 smoothness setting into a catch-up rate per second. */
export function followRate(smoothness: number): number {
  if (smoothness <= 0.001) return Infinity;
  // 0.05 -> ~48/s (a few ms behind), 1 -> 5/s (a slow glide).
  return 5 + (1 - smoothness) ** 2 * 50;
}

export function step(pos: Vec, target: Vec, dtSeconds: number, smoothness: number): Vec {
  const rate = followRate(smoothness);
  if (!isFinite(rate)) return { ...target };
  const k = 1 - Math.exp(-rate * Math.min(dtSeconds, 0.1));
  const nx = pos.x + (target.x - pos.x) * k;
  const ny = pos.y + (target.y - pos.y) * k;
  // Snap when within a fraction of a pixel so the loop can go idle.
  if (Math.abs(target.x - nx) < 0.25 && Math.abs(target.y - ny) < 0.25) return { ...target };
  return { x: nx, y: ny };
}

/**
 * Where the buddy's top-left corner goes for a cursor at `c`, keeping it inside
 * `bounds` by flipping to the other side of the cursor near an edge.
 */
export function placement(c: Vec, offset: Vec, size: number, bounds: { width: number; height: number }) {
  let x = c.x + offset.x;
  let y = c.y + offset.y;
  let flipped = false;
  if (x + size > bounds.width || x < 0) {
    x = c.x - offset.x - size;
    flipped = true;
  }
  if (y + size > bounds.height || y < 0) y = c.y - offset.y - size;
  return { x, y, flipped };
}
