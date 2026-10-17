// Mock data for looking at the island outside the app, in a plain browser:
// island.html?mock=<scene> during `npm run dev`. Never active in a build.
import type { AgentView } from "../bindings/AgentView";
import type { NotchInfo } from "../bindings/NotchInfo";
import type { Settings } from "../bindings/Settings";
import type { UsageToday } from "../bindings/UsageToday";
import type { VoicePhase } from "../bindings/VoicePhase";

export type Demo = {
  settings: Settings;
  notch: NotchInfo | null;
  agents: AgentView[];
  voice: VoicePhase;
  speaking: boolean;
  asking: boolean;
  cheer: boolean;
  sleepy: boolean;
  open: boolean;
  usage: UsageToday | null;
};

function agent(id: string, name: string, goal: string, over: Partial<AgentView>): AgentView {
  return {
    id, batch: "b1", order: 0, name, goal, tools: [], keepOpen: false, after: [], parent: null, status: "running", statusLine: "",
    plan: [], stop: null, result: null, suggestions: [], error: null, pending: null, unseen: false, dismissed: false, model: null,
    created: Date.now(), finished: null, activeMs: 0, counters: { steps: 0, toolCalls: 0, tokens: 0, cost: 0, costKnown: true, extraTokens: 0, extraCost: 0 },
    maxSteps: 40, log: [], changes: 0, files: [], ...over,
  };
}

const done = (n: number, total: number) => [...Array(total)].map((_, i) => ({ text: `Step ${i + 1}`, status: (i < n ? "done" : "todo") as never }));

function build(scene: string): Demo {
  const settings = {
    buddy: { enabled: true, style: "pip", home: "notch", notchEdge: "top", notchOffset: 0.5, islandLook: scene === "peek" ? "peek" : "face", islandSize: 1, size: 34, opacity: 1, offsetX: 18, offsetY: 20, smoothness: 0.35, showStateAnimations: true, showAgentBadge: true, hideInFullscreen: true, hideWhenIdle: true, idleSeconds: 2 },
    hotkeys: { voiceAsk: "Alt+Shift+Space", textAsk: "Alt+Shift+A", circleToExplain: "Alt+Shift+C" },
    agents: { dockSide: "right" },
  } as unknown as Settings;
  const d: Demo = { settings, notch: { width: 185, height: 32 }, agents: [], voice: { phase: "idle", message: null }, speaking: false, asking: false, cheer: false, sleepy: false, open: scene !== "rest" && scene !== "peek" && scene !== "sleepy", usage: { calls: 14, tokens: 48210, inputTokens: 40000, outputTokens: 8210, cacheReadTokens: 0, cacheWriteTokens: 0, cost: 0.083 } };
  const laptops = agent("a1", "Laptops", "Compare three laptops under $1200 for video editing", { statusLine: "Reading three reviews", plan: done(2, 5) });
  const inbox = agent("a2", "Inbox", "Sort this week's newsletters", { statusLine: "Sorting 38 emails", plan: done(1, 3) });
  switch (scene) {
    case "agents":
      d.agents = [laptops, inbox, agent("a3", "Report", "Write the weekly report", { status: "done", finished: Date.now() - 60_000, plan: done(4, 4) })];
      break;
    case "approval":
      d.agents = [agent("a4", "Expenses", "File the receipts in Drive", { status: "approval", statusLine: "Waiting for your OK", pending: { type: "approval", id: "p1", kind: "write" as never, source: "Files", summary: "Move 12 receipts to Drive/Expenses", detail: "", args: {}, editable: [] }, plan: done(2, 4) }), laptops, inbox, agent("a5", "Notes", "Tidy meeting notes", { statusLine: "Reading notes" })];
      break;
    case "listening":
      d.voice = { phase: "listening" };
      break;
    case "thinking":
      d.voice = { phase: "thinking", transcript: "Which of these laptops is best for video editing?" };
      break;
    case "speaking":
      d.speaking = true;
      break;
    case "cheer":
      d.cheer = true;
      d.agents = [agent("a3", "Report", "Write the weekly report", { status: "done", finished: Date.now() - 60_000, plan: done(4, 4) })];
      break;
    case "sleepy":
      d.sleepy = true;
      break;
    case "rest":
    case "peek":
      // Idle it hides in the notch; the strip shows while agents work.
      d.agents = [laptops];
      break;
    case "flat":
      d.notch = null;
      break;
  }
  return d;
}

const scene = import.meta.env.DEV ? new URLSearchParams(location.search).get("mock") : null;
export const demo: Demo | null = scene ? build(scene) : null;
if (demo) document.body.classList.add("demo");
