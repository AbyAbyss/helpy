import { describe, expect, it } from "vitest";
import { modelFromInfo, newProvider } from "./presets";

describe("provider presets", () => {
  it("gives new providers unique ids and names", () => {
    const a = newProvider("ollama", []);
    const b = newProvider("ollama", [a]);
    expect([a.id, b.id]).toEqual(["ollama", "ollama-2"]);
    expect(b.name).toBe("Ollama 2");
    expect(newProvider("anthropic", []).models[0].id).toBe("claude-opus-5");
  });

  it("fills known prices, frees local models, and keeps unknown vision off", () => {
    expect(modelFromInfo("anthropic", { id: "claude-sonnet-5", vision: true, tools: true })).toMatchObject({ inputPrice: 2, outputPrice: 10 });
    expect(modelFromInfo("ollama", { id: "llava", vision: true, tools: false })).toMatchObject({ inputPrice: 0, outputPrice: 0, tools: false });
    expect(modelFromInfo("openAiCompatible", { id: "x", vision: null, tools: null })).toMatchObject({ vision: false, tools: true, inputPrice: null });
  });
});
