import type { AskAction } from "../bindings/AskAction";
import type { AskEvent } from "../bindings/AskEvent";

export type Item =
  | { kind: "user"; text: string; voice: boolean }
  /** `final` once its model call finished; a retry discards non-final text. */
  | { kind: "assistant"; text: string; final: boolean }
  | { kind: "screen"; thumbnail: string; monitor: string }
  | { kind: "step"; number: number; total: number | null; text: string }
  | { kind: "permission"; id: number; answer: "allowed" | "denied" | null }
  | { kind: "retry"; text: string }
  | { kind: "notice"; text: string }
  | { kind: "error"; text: string; action: AskAction | null };

const withoutRetry = (items: Item[]) => items.filter((i) => i.kind !== "retry");

/** Drops the unfinished answer from a failed attempt. */
const withoutPartial = (items: Item[]) => items.filter((i) => !(i.kind === "assistant" && !i.final));

function seconds(ms: number) {
  return ms < 1000 ? "now" : `in ${Math.round(ms / 1000)} s`;
}

export function apply(items: Item[], e: AskEvent): Item[] {
  switch (e.type) {
    case "question":
      return [...(e.fresh ? [] : items), { kind: "user", text: e.text, voice: e.voice }];
    case "started":
      return items;
    case "text": {
      const last = items[items.length - 1];
      if (last?.kind === "assistant" && !last.final) {
        return [...items.slice(0, -1), { ...last, text: last.text + e.text }];
      }
      return [...withoutRetry(items), { kind: "assistant", text: e.text, final: false }];
    }
    case "retry": {
      const text = `Retry ${e.retry} of ${e.limit}: ${e.reason}. Trying ${e.model} ${seconds(e.wait)}…`;
      return [...withoutRetry(withoutPartial(items)), { kind: "retry", text }];
    }
    case "checkpoint":
      return withoutRetry(items).map((i) => (i.kind === "assistant" && !i.final ? { ...i, final: true } : i));
    case "screenPermission":
      return [...items, { kind: "permission", id: e.id, answer: null }];
    case "screen":
      return [...items, { kind: "screen", thumbnail: e.thumbnail, monitor: e.monitor }];
    case "step":
      return [...items, { kind: "step", number: e.number, total: e.total, text: e.instruction }];
    case "notice":
      return [...items, { kind: "notice", text: e.message }];
    case "done":
      return withoutRetry(items);
    case "error":
      return [...withoutRetry(withoutPartial(items)), { kind: "error", text: e.message, action: e.action }];
  }
}

/** Whether to show the "thinking" dots: running and nothing streaming yet. */
export function waiting(items: Item[], running: boolean) {
  if (!running) return false;
  const last = items[items.length - 1];
  return !last || (last.kind !== "assistant" && last.kind !== "retry" && !(last.kind === "permission" && last.answer === null));
}
