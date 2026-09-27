import { describe, expect, it } from "vitest";
import { arrow, bubbleBelow, dimPath, inkFor, labelAbove, lineLabelAt, linePath, pointAlong, sampleLine } from "./geometry";

describe("guidance geometry", () => {
  it("picks readable text on light and dark highlight colours", () => {
    expect(inkFor("#e5484d")).toBe("#ffffff");
    expect(inkFor("#ffd60a")).toBe("#12151c");
    expect(inkFor("#1b1f2a")).toBe("#ffffff");
    expect(inkFor("nonsense")).toBe("#ffffff");
  });

  it("points the arrow head at the target", () => {
    const a = arrow({ x: 0, y: 100 }, { x: 200, y: 100 }, false, 3);
    expect(a.head[0]).toEqual({ x: 200, y: 100 });
    // A straight horizontal arrow: the head's base is behind the tip, split evenly.
    expect(a.head[1].x).toBeLessThan(200);
    expect(a.head[1].y + a.head[2].y).toBeCloseTo(200);
    expect(a.shaft.startsWith("M 0.0 100.0 L")).toBe(true);
  });

  it("bows curved arrows upward whichever way they go", () => {
    for (const [from, to] of [
      [{ x: 0, y: 300 }, { x: 400, y: 300 }],
      [{ x: 400, y: 300 }, { x: 0, y: 300 }],
    ]) {
      const a = arrow(from, to, true, 3);
      const control = a.shaft.match(/Q ([\d.-]+) ([\d.-]+)/)!;
      expect(Number(control[2])).toBeLessThan(300);
    }
    // Too short to curve.
    expect(arrow({ x: 0, y: 0 }, { x: 20, y: 0 }, true, 3).shaft).toContain(" L ");
  });

  it("flips labels and bubbles near the top edge", () => {
    expect(labelAbove(200)).toBe(true);
    expect(labelAbove(10)).toBe(false);
    expect(bubbleBelow(20)).toBe(true);
    expect(bubbleBelow(300)).toBe(false);
  });

  it("cuts one hole per highlight out of the dim layer", () => {
    const d = dimPath(100, 100, [
      { x: 10, y: 10, width: 20, height: 20 },
      { x: 50, y: 50, width: 20, height: 20 },
    ]);
    expect(d.match(/Z/g)).toHaveLength(3);
    expect(d.startsWith("M0 0H100V100H0Z")).toBe(true);
  });

  it("draws lines straight or as a curve through every point", () => {
    const pts = [
      { x: 0, y: 0 },
      { x: 100, y: 0 },
      { x: 100, y: 100 },
    ];
    expect(linePath(pts, false, false)).toBe("M 0.0 0.0 L 100.0 0.0 L 100.0 100.0");
    expect(linePath(pts, true, false).endsWith("Z")).toBe(true);
    const curve = linePath(pts, false, true);
    expect(curve.match(/ C /g)).toHaveLength(2);
    // A curve's samples pass through each point.
    const s = sampleLine(pts, false, true, 8);
    expect(s[8]).toEqual({ x: 100, y: 0 });
    expect(s[s.length - 1]).toEqual({ x: 100, y: 100 });
    // A closed straight shape comes back to its start.
    const closed = sampleLine(pts, true, false);
    expect(closed[closed.length - 1]).toEqual({ x: 0, y: 0 });
  });

  it("labels an open line at its middle and a shape at its centre", () => {
    expect(lineLabelAt([{ x: 0, y: 0 }, { x: 200, y: 0 }], false, false)).toEqual({ x: 100, y: 0 });
    expect(lineLabelAt([{ x: 0, y: 0 }, { x: 90, y: 0 }, { x: 0, y: 90 }], true, false)).toEqual({ x: 30, y: 30 });
    expect(pointAlong([{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }], 0.75)).toEqual({ x: 10, y: 5 });
  });
});
