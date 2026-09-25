import { describe, expect, it } from "vitest";
import { followStep } from "./follow";

describe("followStep", () => {
  it("snaps when smoothness is 0", () => {
    expect(followStep({ x: 0, y: 0 }, { x: 100, y: 50 }, 0, 16.7)).toEqual({ x: 100, y: 50 });
  });

  it("covers the same distance per second at 60 Hz and 144 Hz", () => {
    let a = { x: 0, y: 0 };
    for (let i = 0; i < 60; i++) a = followStep(a, { x: 1000, y: 0 }, 0.9, 1000 / 60);
    let b = { x: 0, y: 0 };
    for (let i = 0; i < 144; i++) b = followStep(b, { x: 1000, y: 0 }, 0.9, 1000 / 144);
    expect(Math.abs(a.x - b.x)).toBeLessThan(0.5);
  });

  it("settles exactly on the target", () => {
    let p = { x: 0, y: 0 };
    for (let i = 0; i < 400; i++) p = followStep(p, { x: 10, y: 10 }, 0.5, 16.7);
    expect(p).toEqual({ x: 10, y: 10 });
  });
});
