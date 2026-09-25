import type { AskEvent } from "../bindings/AskEvent";
import type { VoicePhase } from "../bindings/VoicePhase";

/** Everything the pill shows, derived from voice and answer events. */
export type PillState = {
  mode: "listening" | "working" | "answering" | "done" | "message";
  /** Live transcript, the question, or the answer so far. */
  caption: string;
  tone: "normal" | "error";
  /** A short line under the caption, e.g. how to hand the task to agents. */
  hint?: string;
};

export const initial: PillState = { mode: "listening", caption: "", tone: "normal" };

export function onVoice(s: PillState, p: VoicePhase): PillState {
  switch (p.phase) {
    case "listening":
      return initial;
    case "transcribing":
      return { ...s, mode: "working" };
    case "thinking":
      return { mode: "working", caption: `“${p.transcript}”`, tone: "normal" };
    case "idle":
      return p.message ? { mode: "message", caption: p.message, tone: "normal" } : s;
    case "error":
      return { mode: "message", caption: p.message, tone: "error" };
  }
}

export function onPartial(s: PillState, text: string): PillState {
  return s.mode === "listening" ? { ...s, caption: text } : s;
}

/** Only voice questions drive the pill; typed ones belong to the panel. */
export function onAsk(s: PillState, e: AskEvent, voiceTurn: boolean): PillState {
  if (!voiceTurn) return s;
  switch (e.type) {
    case "text":
      return { mode: "answering", caption: s.mode === "answering" ? s.caption + e.text : e.text, tone: "normal" };
    case "retry":
      return { mode: "working", caption: s.caption, tone: "normal" };
    case "done":
      return { ...s, mode: "done", ...(e.offerAgents ? { hint: "Say “do it” to have agents do this" } : {}) };
    case "error":
      return { mode: "message", caption: e.message, tone: "error" };
    default:
      return s;
  }
}

/** Plain text for the caption: no Markdown symbols, first lines only. */
export function captionText(markdown: string, max = 160): string {
  const plain = markdown
    .replace(/```[\s\S]*?(```|$)/g, "")
    .replace(/[*_`#>]/g, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\s+/g, " ")
    .trim();
  return plain.length > max ? `${plain.slice(0, max - 1).trimEnd()}…` : plain;
}
