import type { AgentView } from "../bindings/AgentView";
import type { Status } from "../bindings/Status";

/** Status colours shared by the dock, cards and panel (R2). */
export type Tone = "work" | "done" | "question" | "alert" | "idle";

export function tone(s: Status): Tone {
  switch (s) {
    case "running":
      return "work";
    case "done":
    case "ready":
      return "done";
    case "question":
      return "question";
    case "approval":
    case "failed":
    case "stopped":
      return "alert";
    default:
      return "idle";
  }
}

export const STATUS_LABEL: Record<Status, string> = {
  queued: "Waiting",
  running: "Working",
  approval: "Needs your OK",
  question: "Question",
  paused: "Paused",
  ready: "Ready for changes",
  done: "Done",
  failed: "Failed",
  stopped: "Stopped",
  cancelled: "Cancelled",
};

const finished = (s: Status) => s === "done" || s === "failed" || s === "stopped" || s === "cancelled";

/**
 * Which agents sit in the dock, oldest first: everything active, anything
 * that needs the user, open-ended agents ready for changes, and finished
 * ones for `doneSeconds` (0 keeps them). Failures stay until dismissed.
 */
export function inDock(agents: AgentView[], now: number, doneSeconds: number): AgentView[] {
  return agents
    .filter((a) => {
      if (a.dismissed) return false;
      if (!finished(a.status)) return true;
      if (a.status === "failed" || a.status === "stopped") return true;
      if (doneSeconds === 0) return true;
      return a.finished != null && now - a.finished < doneSeconds * 1000;
    })
    .sort((a, b) => a.created - b.created || a.order - b.order);
}

export function duration(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}

export function tokens(n: number): string {
  if (n < 1000) return `${n}`;
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

/** "3m 12s · 18k tokens · $0.04". Time is what the agent spent working. */
export function spend(a: AgentView): string {
  const parts = [duration(a.activeMs), `${tokens(a.counters.tokens)} tokens`];
  if (a.counters.costKnown && a.counters.cost > 0) parts.push(`$${a.counters.cost.toFixed(a.counters.cost < 0.1 ? 3 : 2)}`);
  return parts.join(" · ");
}

/** The latest thing the agent ran, for the monospace line on the card. */
export function lastCommand(a: AgentView): string | null {
  for (let i = a.log.length - 1; i >= 0; i--) if (a.log[i].kind === "tool") return a.log[i].text;
  return null;
}
