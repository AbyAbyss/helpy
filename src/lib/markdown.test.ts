import { describe, expect, it } from "vitest";
import { mendChart } from "./markdown";

describe("mendChart", () => {
  it("brackets and quotes bare x-axis categories", () => {
    const src = 'xychart-beta\n  title "Prices"\n  x-axis Yonex Nanoray 700FX, Li-Ning "Pro" 8000\n  bar [119.99, 139.95]';
    expect(mendChart(src)).toBe('xychart-beta\n  title "Prices"\n  x-axis ["Yonex Nanoray 700FX", "Li-Ning \'Pro\' 8000"]\n  bar [119.99, 139.95]');
  });

  it("leaves correct charts and other diagrams alone", () => {
    for (const src of ['xychart-beta\n  x-axis ["A", "B"]\n  bar [1, 2]', 'xychart-beta\n  x-axis "Year" 2020 --> 2025', "pie\n  \"A, B\" : 1"]) {
      expect(mendChart(src)).toBe(src);
    }
  });
});
