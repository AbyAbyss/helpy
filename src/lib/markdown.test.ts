import { describe, expect, it } from "vitest";
import { mendChart, normalizeMath } from "./markdown";

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

describe("normalizeMath", () => {
  it("keeps dollar amounts as money", () => {
    expect(normalizeMath("£250 is about $329.79 at $1.3191 to the pound")).toBe("£250 is about \\$329.79 at \\$1.3191 to the pound");
  });

  it("drops leaked citation markers", () => {
    expect(normalizeMath('workloads 【{"id":0,"cursor":0,"loc":0}】.')).toBe("workloads.");
  });

  it("turns <br> in table cells into spaces", () => {
    expect(normalizeMath("| • Solid<br>• Stable<br/> |")).toBe("| • Solid • Stable  |");
  });

  it("leaves math and display math alone", () => {
    expect(normalizeMath("so $x^2$ and")).toBe("so $x^2$ and");
    expect(normalizeMath("$$2x$$")).toBe("$$\n2x\n$$");
  });
});
