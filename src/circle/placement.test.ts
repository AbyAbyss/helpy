import { describe, expect, it } from "vitest";
import { boundsOf, nearestOnRect, overlaps, placeCard, placeLabels, type Rect } from "./placement";

const SCREEN = { w: 1920, h: 1080 };

describe("circle placement", () => {
  it("puts the card right of the selection, or left when there's no room", () => {
    const card = { w: 360, h: 400 };
    expect(placeCard({ x: 200, y: 300, w: 400, h: 300 }, card, SCREEN)).toMatchObject({ x: 616, y: 300 });
    const left = placeCard({ x: 1400, y: 300, w: 400, h: 300 }, card, SCREEN);
    expect(left.x + left.w).toBe(1384);
    // Near the bottom, it slides up to stay on screen.
    expect(placeCard({ x: 200, y: 900, w: 300, h: 100 }, card, SCREEN).y).toBe(1080 - 400 - 8);
    // Everything selected: over the bottom-right corner, still on screen.
    const full = placeCard({ x: 0, y: 0, w: 1920, h: 1080 }, card, SCREEN);
    expect(full.x + full.w).toBeLessThanOrEqual(1920);
  });

  it("labels never overlap each other, the selection, the card or the screen edge", () => {
    const sel = { x: 700, y: 350, w: 400, h: 380 };
    const card = placeCard(sel, { w: 360, h: 420 }, SCREEN);
    // Eight parts, several bunched in the same corner.
    const anchors = [
      { x: 720, y: 360 }, { x: 730, y: 370 }, { x: 740, y: 365 }, { x: 900, y: 360 },
      { x: 1090, y: 540 }, { x: 900, y: 720 }, { x: 710, y: 700 }, { x: 900, y: 540 },
    ];
    const sizes = anchors.map((_, i) => ({ w: 90 + i * 12, h: 30 }));
    const labels = placeLabels(sel, anchors, sizes, SCREEN, [card]);
    const placed = labels.filter((l): l is Rect => l !== null);
    expect(placed.length).toBe(anchors.length);
    for (const [i, a] of placed.entries()) {
      expect(overlaps(a, sel)).toBe(false);
      expect(overlaps(a, card)).toBe(false);
      expect(a.x >= 0 && a.y >= 0 && a.x + a.w <= SCREEN.w && a.y + a.h <= SCREEN.h).toBe(true);
      for (const b of placed.slice(i + 1)) expect(overlaps(a, b)).toBe(false);
    }
  });

  it("puts a label on the side its part is on", () => {
    const sel = { x: 700, y: 350, w: 400, h: 300 };
    const [left, right] = placeLabels(sel, [{ x: 710, y: 500 }, { x: 1090, y: 500 }], [{ w: 80, h: 28 }, { w: 80, h: 28 }], SCREEN);
    expect(left!.x + left!.w).toBeLessThanOrEqual(sel.x);
    expect(right!.x).toBeGreaterThanOrEqual(sel.x + sel.w);
  });

  it("gives up on a label with nowhere to go instead of overlapping", () => {
    const sel = { x: 8, y: 8, w: 1904, h: 1064 };
    expect(placeLabels(sel, [{ x: 500, y: 500 }], [{ w: 80, h: 28 }], SCREEN)).toEqual([null]);
  });

  it("finds leader-line ends and drawn bounds", () => {
    expect(nearestOnRect({ x: 0, y: 50 }, { x: 10, y: 0, w: 20, h: 100 })).toEqual({ x: 10, y: 50 });
    expect(boundsOf([{ x: 5, y: 9 }, { x: 1, y: 20 }, { x: 8, y: 2 }])).toEqual({ x: 1, y: 2, w: 7, h: 18 });
  });
});
