import { describe, expect, it } from "vitest";
import type { AgentView } from "../bindings/AgentView";
import { duration, inDock, lastCommand, spend, tokens, tone } from "./dockState";

const agent = (over: Partial<AgentView>): AgentView => ({
  id: "a",
  batch: "b",
  order: 0,
  name: "Research",
  goal: "g",
  tools: [],
  keepOpen: false,
  after: [],
  parent: null,
  files: [],
  status: "running",
  statusLine: "",
  stop: null,
  result: null,
  suggestions: [],
  error: null,
  pending: null,
  unseen: false,
  dismissed: false,
  created: 0,
  finished: null,
  activeMs: 0,
  counters: { steps: 0, toolCalls: 0, tokens: 0, cost: 0, costKnown: true, extraTokens: 0, extraCost: 0 },
  maxSteps: 25,
  log: [],
  changes: 0,
  ...over,
});

describe("dock", () => {
  it("colours follow R2: blue working, green done, yellow question, red needs you or failed", () => {
    expect(tone("running")).toBe("work");
    expect(tone("ready")).toBe("done");
    expect(tone("question")).toBe("question");
    expect(tone("approval")).toBe("alert");
    expect(tone("stopped")).toBe("alert");
    expect(tone("paused")).toBe("idle");
  });

  it("keeps finished agents for a while, failures until dismissed", () => {
    const now = 100_000;
    const list = [
      agent({ id: "run", created: 3 }),
      agent({ id: "done-recent", status: "done", finished: now - 10_000, created: 1 }),
      agent({ id: "done-old", status: "done", finished: now - 90_000 }),
      agent({ id: "failed", status: "failed", finished: 0 }),
      agent({ id: "ready", status: "ready", finished: 0 }),
      agent({ id: "gone", dismissed: true }),
    ];
    // Oldest first; the finished-long-ago one and the dismissed one are gone.
    expect(inDock(list, now, 60).map((a) => a.id)).toEqual(["failed", "ready", "done-recent", "run"]);
    expect(inDock(list, now, 0).map((a) => a.id)).toContain("done-old");
  });

  it("formats time, tokens, cost and the last command", () => {
    expect(duration(75_000)).toBe("1m 15s");
    expect(duration(3_900_000)).toBe("1h 5m");
    expect(tokens(18_234)).toBe("18k");
    expect(tokens(4_200)).toBe("4.2k");
    const a = agent({ status: "done", activeMs: 5_000, counters: { steps: 1, toolCalls: 1, tokens: 900, cost: 0.0123, costKnown: true, extraTokens: 0, extraCost: 0 }, log: [{ at: 1, kind: "tool", text: "web_search {\"query\":\"x\"}" }, { at: 2, kind: "result", text: "ok" }] });
    expect(spend(a)).toBe("5s · 900 tokens · $0.012");
    expect(lastCommand(a)).toBe('web_search {"query":"x"}');
  });
});
