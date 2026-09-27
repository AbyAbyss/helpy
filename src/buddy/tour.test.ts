import { describe, expect, it } from "vitest";
import type { Mark } from "../bindings/Mark";
import { easeInOut, handoffFrames, planTour, tourAt, traceOf } from "./tour";

const line = (points: [number, number][], closed = false): Mark => ({ type: "line", points, closed, curved: false, label: null, raw: "" });

describe("buddy tour", () => {
  it("traces a highlight clockwise from its padded top-left corner", () => {
    const t = traceOf({ type: "highlight", x: 100, y: 50, width: 40, height: 20, label: null, raw: "" }, true);
    expect(t[0]).toEqual({ x: 94, y: 44 });
    expect(t[1]).toEqual({ x: 146, y: 44 });
    expect(t[4]).toEqual(t[0]);
  });

  it("follows a curved arrow's bow and ends on its tip", () => {
    const t = traceOf({ type: "arrow", fromX: 0, fromY: 200, toX: 400, toY: 200, label: null, raw: "" }, true);
    expect(t[0]).toEqual({ x: 0, y: 200 });
    expect(t[t.length - 1]).toEqual({ x: 400, y: 200 });
    // Curved arrows arc up the screen.
    expect(t[8].y).toBeLessThan(200);
  });

  it("flies to each mark, then draws it, one after another", () => {
    const tour = planTour([line([[100, 0], [300, 0]]), line([[300, 100], [300, 300]])], { x: 0, y: 0 }, 1, true);
    const [a, b] = tour.legs;
    expect(a.fly).toBe(0);
    expect(a.draw).toBeGreaterThan(0);
    expect(b.fly).toBeGreaterThan(a.draw + a.dur);
    expect(tour.end).toBeGreaterThan(b.draw + b.dur);
    // At the start, on the way, drawing, and at the end.
    expect(tourAt(tour, 0)).toEqual({ x: 0, y: 0 });
    expect(tourAt(tour, a.draw)).toEqual({ x: 100, y: 0 });
    const mid = tourAt(tour, a.draw + a.dur / 2);
    expect(mid.x).toBeCloseTo(200, 0);
    expect(mid.y).toBe(0);
    expect(tourAt(tour, tour.end + 1000)).toEqual({ x: 300, y: 300 });
  });

  it("stretches with a slower animation speed and pauses at pointers", () => {
    const marks: Mark[] = [{ type: "point", x: 500, y: 0, label: null, raw: "" }];
    const fast = planTour(marks, { x: 0, y: 0 }, 1, true);
    const slow = planTour(marks, { x: 0, y: 0 }, 2, true);
    expect(fast.legs[0].dur).toBe(0);
    expect(slow.end).toBeCloseTo(fast.end * 2);
  });

  it("eases like CSS ease-in-out", () => {
    expect(easeInOut(0)).toBe(0);
    expect(easeInOut(1)).toBe(1);
    expect(easeInOut(0.5)).toBeCloseTo(0.5, 3);
    expect(easeInOut(0.25)).toBeCloseTo(0.129, 2);
  });

  it("flies the hand-off copy from the buddy to the chip in an upward arc", () => {
    const f = handoffFrames({ x: 400, y: 600 }, { x: 1880, y: 540 });
    expect(f[0]).toMatchObject({ offset: 0, x: 400, y: 600, opacity: 1 });
    const last = f[f.length - 1];
    expect(last.offset).toBeCloseTo(1);
    expect(last.x).toBeCloseTo(1880);
    expect(last.y).toBeCloseTo(540);
    expect(last.opacity).toBeCloseTo(0);
    expect(last.scale).toBeLessThan(1);
    expect(Math.min(...f.map((p) => p.y))).toBeLessThan(480);
    // Offsets rise, and it never turns more than half a circle between frames.
    for (let i = 1; i < f.length; i++) {
      expect(f[i].offset).toBeGreaterThan(f[i - 1].offset);
      expect(Math.abs(f[i].angle - f[i - 1].angle)).toBeLessThanOrEqual(180);
    }
  });
});
