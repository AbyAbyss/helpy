import type { ChatSummary } from "../bindings/ChatSummary";

export type Kind = "all" | "typed" | "voice";

/** Whether a chat shows under this filter and search. */
export function matches(c: ChatSummary, kind: Kind, query: string) {
  if (kind === "voice" && !c.voice) return false;
  if (kind === "typed" && c.allVoice) return false;
  const q = query.trim().toLowerCase();
  return !q || [c.title, c.preview, c.asked].some((t) => t.toLowerCase().includes(q));
}

/** The heading a chat goes under: Today, Yesterday, this week, then by month. */
export function dayGroup(ms: number, now: Date) {
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const d = new Date(ms);
  const days = Math.round((day(now) - day(d)) / 86_400_000);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return "Previous 7 days";
  return d.toLocaleDateString(undefined, { month: "long", year: d.getFullYear() === now.getFullYear() ? undefined : "numeric" });
}
