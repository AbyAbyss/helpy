import { describe, expect, it } from "vitest";
import defaults from "../../bindings/defaults.json";
import type { Ai } from "../../bindings/Ai";
import { withAutoRouting, withNebiusSetup, withoutProvider } from "./routing";

const base = defaults.ai as Ai;
const m = (id: string, vision: boolean) => ({ id, vision, tools: true, inputPrice: 0, outputPrice: 0 });
const ai: Ai = {
  ...base,
  providers: [
    { id: "local", kind: "ollama", name: "Ollama", baseUrl: "http://localhost:11434/v1", models: [m("text", false), m("llava", true)] },
    { id: "cloud", kind: "anthropic", name: "Anthropic", baseUrl: "https://api.anthropic.com", models: [m("claude-opus-5", true)] },
  ],
};

describe("routing helpers", () => {
  it("picks a vision model for questions when none is set", () => {
    expect(withAutoRouting(ai).routing.ask).toEqual({ providerId: "local", model: "llava" });
  });

  it("keeps an existing choice", () => {
    const set = { ...ai, routing: { ...ai.routing, ask: { providerId: "cloud", model: "claude-opus-5" } } };
    expect(withAutoRouting(set).routing.ask).toEqual({ providerId: "cloud", model: "claude-opus-5" });
  });

  it("removing a provider clears routes and fallbacks that used it", () => {
    const set: Ai = {
      ...ai,
      routing: { ...ai.routing, ask: { providerId: "cloud", model: "claude-opus-5" }, visionFallback: { providerId: "local", model: "llava" } },
      fallbackChain: [{ providerId: "cloud", model: "claude-opus-5" }, { providerId: "local", model: "text" }],
    };
    const next = withoutProvider(set, "cloud");
    expect(next.routing.ask).toBeNull();
    expect(next.routing.visionFallback).toEqual({ providerId: "local", model: "llava" });
    expect(next.fallbackChain).toEqual([{ providerId: "local", model: "text" }]);
  });

  it("sets Nebius up for everything, adding only the models it lacks", () => {
    const kimi = { ...m("moonshotai/Kimi-K3", true), inputPrice: 2 };
    const set: Ai = { ...ai, providers: [...ai.providers, { id: "nebius", kind: "nebius", name: "Nebius", baseUrl: "", models: [kimi] }] };
    const next = withNebiusSetup(set, "nebius");
    const nebius = next.providers.find((p) => p.id === "nebius")!;
    expect(nebius.models.map((x) => x.id)).toEqual(["moonshotai/Kimi-K3", "nvidia/nemotron-3-super-120b-a12b", "nvidia/Nemotron-3-Ultra-550b-a55b"]);
    // The user's own settings for a model it already had are kept.
    expect(nebius.models[0].inputPrice).toBe(2);
    expect(next.routing.ask).toEqual({ providerId: "nebius", model: "nvidia/nemotron-3-super-120b-a12b" });
    expect(next.routing.visionFallback).toEqual({ providerId: "nebius", model: "moonshotai/Kimi-K3" });
    expect(next.routing.agentPlanning).toEqual({ providerId: "nebius", model: "nvidia/Nemotron-3-Ultra-550b-a55b" });
    expect(next.providers.filter((p) => p.id !== "nebius")).toEqual(ai.providers);
  });
});
