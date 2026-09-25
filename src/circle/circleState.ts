import type { AskAction } from "../bindings/AskAction";
import type { CircleAction } from "../bindings/CircleAction";
import type { CircleEvent } from "../bindings/CircleEvent";
import type { LabelPart } from "../bindings/LabelPart";
import type { SelectionShape } from "../bindings/SelectionShape";

/** One action or question about the selection, and its answer. */
export type Turn = {
  action: CircleAction;
  question: string | null;
  text: string;
  status: "working" | "done" | "error";
  retry: string | null;
  error: { message: string; action: AskAction | null } | null;
  copied: number | null;
};

export type CircleView = {
  /** Drawing a selection, or looking at what Helpy says about it. */
  phase: "idle" | "select" | "result";
  shape: SelectionShape;
  problem: string | null;
  turns: Turn[];
  parts: LabelPart[];
};

export const idle: CircleView = { phase: "idle", shape: "rectangle", problem: null, turns: [], parts: [] };

const updateLast = (s: CircleView, f: (t: Turn) => Turn): CircleView =>
  s.turns.length ? { ...s, turns: [...s.turns.slice(0, -1), f(s.turns[s.turns.length - 1])] } : s;

/** Applies a Rust event to the overlay window labelled `self`. */
export function apply(s: CircleView, e: CircleEvent, self: string): CircleView {
  if (e.type === "select") return e.overlay === self ? { ...idle, phase: "select", shape: e.shape, problem: e.problem } : idle;
  if (e.type === "closed") return idle;
  if (s.phase === "idle") return s;
  switch (e.type) {
    case "begin":
      return {
        ...s,
        turns: [...s.turns, { action: e.action, question: e.question, text: "", status: "working", retry: null, error: null, copied: null }],
      };
    case "attempt":
      return updateLast(s, (t) => ({ ...t, text: "", retry: null }));
    case "text":
      return updateLast(s, (t) => ({ ...t, text: t.text + e.text, retry: null }));
    case "retry":
      return updateLast(s, (t) => ({ ...t, retry: `Retry ${e.retry} of ${e.limit}: ${e.reason}` }));
    case "parts":
      return { ...s, parts: e.parts };
    case "copied":
      return updateLast(s, (t) => ({ ...t, copied: e.chars }));
    case "done":
      return updateLast(s, (t) => ({ ...t, status: "done", retry: null }));
    case "error": {
      const error = { message: e.message, action: e.action };
      // An error before anything started (no model) still needs showing.
      if (!s.turns.length || s.turns[s.turns.length - 1].status !== "working") {
        return { ...s, turns: [...s.turns, { action: "explain", question: null, text: "", status: "error", retry: null, error, copied: null }] };
      }
      return updateLast(s, (t) => ({ ...t, status: "error", retry: null, error }));
    }
  }
}

export const running = (s: CircleView) => s.turns.some((t) => t.status === "working");
