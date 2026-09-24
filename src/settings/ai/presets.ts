import type { ModelConfig } from "../../bindings/ModelConfig";
import type { ModelInfo } from "../../bindings/ModelInfo";
import type { ProviderConfig } from "../../bindings/ProviderConfig";
import type { ProviderKind } from "../../bindings/ProviderKind";

export type Preset = {
  kind: ProviderKind;
  name: string;
  baseUrl: string;
  /** "required", "optional" or "none" (local servers). */
  key: "required" | "optional" | "none";
  /** Where to get a key, shown as plain text. */
  keyHint?: string;
  /** Short mark shown on the provider tile. */
  mark: string;
  models: ModelConfig[];
};

const model = (id: string, extra: Partial<ModelConfig> = {}): ModelConfig => ({
  id, vision: true, tools: true, inputPrice: null, outputPrice: null, ...extra,
});

/** Anthropic list prices in USD per million tokens (input, output). */
const CLAUDE_PRICES: Record<string, [number, number]> = {
  "claude-fable-5-1": [10, 50],
  "claude-fable-5": [10, 50],
  "claude-opus-5-5": [4, 20],
  "claude-opus-5": [5, 25],
  "claude-opus-4-8": [5, 25],
  "claude-opus-4-7": [5, 25],
  "claude-opus-4-6": [5, 25],
  "claude-sonnet-5": [2, 10],
  "claude-sonnet-4-6": [3, 15],
  "claude-haiku-4-5": [1, 5],
};

export const PRESETS: Preset[] = [
  {
    kind: "anthropic", name: "Anthropic", baseUrl: "https://api.anthropic.com", key: "required",
    keyHint: "Create a key at console.anthropic.com", mark: "A",
    models: [model("claude-opus-5", { inputPrice: 5, outputPrice: 25 })],
  },
  { kind: "openAi", name: "OpenAI", baseUrl: "https://api.openai.com/v1", key: "required", keyHint: "Create a key at platform.openai.com", mark: "O", models: [] },
  {
    kind: "gemini", name: "Google Gemini", baseUrl: "https://generativelanguage.googleapis.com/v1beta", key: "required",
    keyHint: "Create a key at aistudio.google.com", mark: "G", models: [],
  },
  { kind: "ollama", name: "Ollama", baseUrl: "http://localhost:11434/v1", key: "none", mark: "Ol", models: [] },
  { kind: "lmStudio", name: "LM Studio", baseUrl: "http://localhost:1234/v1", key: "none", mark: "LM", models: [] },
  { kind: "llamaCpp", name: "llama.cpp server", baseUrl: "http://localhost:8080/v1", key: "none", mark: "ll", models: [] },
  { kind: "openAiCompatible", name: "Other (OpenAI-compatible)", baseUrl: "", key: "optional", mark: "</>", models: [] },
];

export const presetFor = (kind: ProviderKind) => PRESETS.find((p) => p.kind === kind)!;

export const isLocal = (kind: ProviderKind) => presetFor(kind).key === "none";

export function newProvider(kind: ProviderKind, existing: ProviderConfig[], baseUrl?: string): ProviderConfig {
  const p = presetFor(kind);
  let n = 1;
  let id = kind.toLowerCase();
  while (existing.some((e) => e.id === id)) id = `${kind.toLowerCase()}-${++n}`;
  return { id, kind, name: n > 1 ? `${p.name} ${n}` : p.name, baseUrl: baseUrl ?? p.baseUrl, models: p.models.map((m) => ({ ...m })) };
}

/** A model config from what the provider reported, with known prices filled in. */
export function modelFromInfo(kind: ProviderKind, info: ModelInfo): ModelConfig {
  const price = kind === "anthropic" ? CLAUDE_PRICES[info.id] : undefined;
  const free = isLocal(kind) ? 0 : null;
  return {
    id: info.id,
    // Unknown vision stays off: sending images to a text-only model fails.
    vision: info.vision ?? false,
    tools: info.tools ?? !isLocal(kind),
    inputPrice: price?.[0] ?? free,
    outputPrice: price?.[1] ?? free,
  };
}
