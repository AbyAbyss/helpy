import { describe, expect, it } from "vitest";
import type { CircleEvent } from "../bindings/CircleEvent";
import { apply, idle, running, type CircleView } from "./circleState";

const run = (events: CircleEvent[], self = "overlay-1-0", start: CircleView = idle) => events.reduce((s, e) => apply(s, e, self), start);
const select: CircleEvent = { type: "select", overlay: "overlay-1-0", shape: "freehand", problem: null };

describe("circle state", () => {
  it("only the overlay being selected on reacts", () => {
    expect(run([select], "overlay-1-1")).toEqual(idle);
    expect(run([select]).phase).toBe("select");
    expect(run([select, { type: "text", text: "x" }]).turns).toEqual([]);
  });

  it("streams an answer, dropping a failed attempt's text", () => {
    const s = run([
      select,
      { type: "begin", action: "explain", question: null },
      { type: "attempt", model: "a" },
      { type: "text", text: "Half" },
      { type: "retry", retry: 1, limit: 3, reason: "Server error", wait: 2000 },
      { type: "attempt", model: "b" },
      { type: "text", text: "A heart " },
      { type: "text", text: "diagram." },
    ]);
    expect(running(s)).toBe(true);
    expect(s.turns[0].text).toBe("A heart diagram.");
    const done = apply(s, { type: "done", model: "b" }, "overlay-1-0");
    expect(running(done)).toBe(false);
    expect(done.turns[0].status).toBe("done");
  });

  it("keeps parts, copies and errors", () => {
    const s = run([
      select,
      { type: "error", message: "No model", action: "openProviders" },
      { type: "begin", action: "copyText", question: null },
      { type: "copied", chars: 12 },
      { type: "parts", parts: [{ label: "Aorta", note: null, detail: null, x: 1, y: 2 }] },
    ]);
    expect(s.turns[0].error).toEqual({ message: "No model", action: "openProviders" });
    expect(s.turns[1].copied).toBe(12);
    expect(s.parts).toHaveLength(1);
    expect(apply(s, { type: "closed" }, "overlay-1-0")).toEqual(idle);
  });
});
