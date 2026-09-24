// Frame-rate independent smoothing for the cursor buddy.

export type Point = { x: number; y: number };

/**
 * Moves `current` toward `target`. `smoothness` is the share of the remaining
 * distance left after one 60 Hz frame: 0 snaps instantly, 0.95 trails far.
 */
export function followStep(current: Point, target: Point, smoothness: number, dtMs: number): Point {
  if (smoothness <= 0) return { ...target };
  const remaining = Math.pow(smoothness, dtMs / (1000 / 60));
  const next = {
    x: target.x + (current.x - target.x) * remaining,
    y: target.y + (current.y - target.y) * remaining,
  };
  // Settle exactly once within a fraction of a pixel, so it stops repainting.
  if (Math.abs(next.x - target.x) < 0.05 && Math.abs(next.y - target.y) < 0.05) return { ...target };
  return next;
}
