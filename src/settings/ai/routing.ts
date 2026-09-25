import type { Ai } from "../../bindings/Ai";
import type { ModelConfig } from "../../bindings/ModelConfig";
import type { ModelRef } from "../../bindings/ModelRef";
import type { Routing } from "../../bindings/Routing";

export const ROUTES: { key: keyof Routing; label: string; built: boolean; help?: string }[] = [
  { key: "ask", label: "Questions (voice and text)", built: true },
  { key: "visionFallback", label: "Screen questions", built: true, help: "Used when the questions model can't read images." },
  {
    key: "visualGuidance",
    label: "Visual guidance",
    built: true,
    help: "Runs walkthroughs after the first step. Needs a model that can read images. Empty uses the questions model.",
  },
  {
    key: "circleToExplain",
    label: "Circle to explain",
    built: true,
    help: "Needs a model that can read images. Empty uses the questions model.",
  },
  { key: "agentPlanning", label: "Agent planning", built: true, help: "Turns a request into agents. Empty uses the questions model." },
  { key: "agentOrchestrator", label: "Agent orchestrators", built: false },
  { key: "agentWorker", label: "Agent workers", built: true, help: "Does the agents' work. Needs a model that can use tools. Empty uses the questions model." },
];

export const refKey = (r: ModelRef) => `${r.providerId}\u0000${r.model}`;

export function allModels(ai: Ai): { ref: ModelRef; model: ModelConfig; provider: string }[] {
  return ai.providers.flatMap((p) => p.models.map((m) => ({ ref: { providerId: p.id, model: m.id }, model: m, provider: p.name })));
}

const exists = (ai: Ai, r: ModelRef | null) => !!r && allModels(ai).some((m) => refKey(m.ref) === refKey(r));

/** Drops routes and fallbacks that point at models which no longer exist. */
export function pruneRefs(ai: Ai): Ai {
  const routing = Object.fromEntries(
    Object.entries(ai.routing).map(([k, r]) => [k, exists(ai, r as ModelRef | null) ? r : null]),
  ) as Routing;
  return { ...ai, routing, fallbackChain: ai.fallbackChain.filter((r) => exists(ai, r)) };
}

export function withoutProvider(ai: Ai, id: string): Ai {
  return pruneRefs({ ...ai, providers: ai.providers.filter((p) => p.id !== id) });
}

/**
 * Keeps references valid after a provider edit, and picks a questions model
 * when none is set yet: the first model that can see and use tools, else the
 * first model.
 */
export function withAutoRouting(ai: Ai): Ai {
  const next = pruneRefs(ai);
  if (next.routing.ask) return next;
  const models = allModels(next);
  const best = models.find((m) => m.model.vision && m.model.tools) ?? models[0];
  return best ? { ...next, routing: { ...next.routing, ask: best.ref } } : next;
}
