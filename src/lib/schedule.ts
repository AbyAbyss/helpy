// Trigger schedules (5-field cron, local time) in plain words, shared by
// the settings editor and the plan card.

export type Repeat =
  | { kind: "daily"; time: string }
  | { kind: "weekdays"; time: string }
  | { kind: "weekly"; days: number[]; time: string }
  | { kind: "hours"; every: number }
  | { kind: "minutes"; every: number }
  | { kind: "custom"; cron: string };

export const DAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

const pad = (n: number) => String(n).padStart(2, "0");

/** "0 9 * * 1-5" → weekdays at 09:00; anything else is custom. */
export function fromCron(cron: string): Repeat {
  const f = cron.trim().split(/\s+/);
  if (f.length !== 5) return { kind: "custom", cron };
  const [m, h, dom, mon, dow] = f;
  const num = (s: string) => /^\d+$/.test(s);
  if (dom === "*" && mon === "*") {
    if (num(m) && num(h)) {
      const time = `${pad(+h)}:${pad(+m)}`;
      if (dow === "*") return { kind: "daily", time };
      if (dow === "1-5") return { kind: "weekdays", time };
      if (/^[0-7](,[0-7])*$/.test(dow)) return { kind: "weekly", time, days: [...new Set(dow.split(",").map((d) => +d % 7))].sort() };
    }
    if (m === "0" && dow === "*" && /^\*\/\d+$/.test(h)) return { kind: "hours", every: +h.slice(2) };
    if (h === "*" && dow === "*" && /^\*\/\d+$/.test(m)) return { kind: "minutes", every: +m.slice(2) };
  }
  return { kind: "custom", cron };
}

export function toCron(r: Repeat): string {
  const hm = (t: string) => {
    const [h, m] = t.split(":").map((x) => +x || 0);
    return `${m} ${h}`;
  };
  switch (r.kind) {
    case "daily":
      return `${hm(r.time)} * * *`;
    case "weekdays":
      return `${hm(r.time)} * * 1-5`;
    case "weekly":
      return `${hm(r.time)} * * ${(r.days.length ? r.days : [1]).join(",")}`;
    case "hours":
      return `0 */${r.every} * * *`;
    case "minutes":
      return `*/${r.every} * * * *`;
    case "custom":
      return r.cron;
  }
}

export function describe(cron: string): string {
  const r = fromCron(cron);
  switch (r.kind) {
    case "daily":
      return `Every day at ${r.time}`;
    case "weekdays":
      return `Weekdays at ${r.time}`;
    case "weekly":
      return `Every ${r.days.map((d) => DAY_NAMES[d]).join(", ")} at ${r.time}`;
    case "hours":
      return r.every === 1 ? "Every hour" : `Every ${r.every} hours`;
    case "minutes":
      return `Every ${r.every} minutes`;
    case "custom":
      return `Schedule "${r.cron}"`;
  }
}

/** A one-time run's moment in plain words: "Today at 14:35", "Tomorrow at 09:00", "Tue 20 Oct at 09:00". */
export function describeOnce(ms: number): string {
  const d = new Date(ms);
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const today = new Date();
  const tomorrow = new Date(today);
  tomorrow.setDate(today.getDate() + 1);
  if (d.toDateString() === today.toDateString()) return `Today at ${time}`;
  if (d.toDateString() === tomorrow.toDateString()) return `Tomorrow at ${time}`;
  return `${d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })} at ${time}`;
}

/** A moment as an <input type="datetime-local"> value, in local time. */
export function toLocalInput(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** The input's value back as a moment; null when it isn't one. */
export function fromLocalInput(value: string): number | null {
  const t = new Date(value).getTime();
  return Number.isFinite(t) ? t : null;
}
