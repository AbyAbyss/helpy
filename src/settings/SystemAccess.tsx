import { useEffect, useState } from "react";
import type { Access } from "../bindings/Access";
import type { Permissions } from "../bindings/Permissions";
import { api } from "../lib/ipc";

type Which = "microphone" | "screen" | "accessibility";

const ROWS: { which: Which; name: string; why: string }[] = [
  { which: "microphone", name: "Microphone", why: "To hear you when you hold the voice shortcut." },
  { which: "screen", name: "Screen Recording", why: "To see what you ask about." },
  {
    which: "accessibility",
    name: "Accessibility",
    why: "To point at the exact button, click for you with “Do it for me”, and move, focus and arrange app windows.",
  },
];

const PILL: Record<Access, [string, string]> = {
  allowed: ["pill--ok", "Allowed"],
  denied: ["pill--err", "Off"],
  notAsked: ["pill--idle", "Not asked yet"],
};

/** The permissions turned off, by name; never-asked ones get asked for when needed. */
export function missing(p: Permissions) {
  return ROWS.filter((r) => p[r.which] === "denied").map((r) => r.name);
}

/** What the system lets Helpy do, kept current while it's on screen. */
export function SystemAccess({ onError }: { onError: (m: string) => void }) {
  const [perms, setPerms] = useState<Permissions | null>(null);
  useEffect(() => {
    const load = () => document.visibilityState === "visible" && api.permissions().then(setPerms, () => {});
    load();
    // Picks up a switch flipped in System Settings without a click here.
    const t = setInterval(load, 2000);
    window.addEventListener("focus", load);
    return () => {
      clearInterval(t);
      window.removeEventListener("focus", load);
    };
  }, []);
  if (!perms) return null;
  const rows = ROWS.filter((r) => perms[r.which] !== null);
  if (rows.length === 0) return <p className="page__note">This system has no permissions to grant. Everything works as it is.</p>;
  const run = (f: () => Promise<unknown>) => f().catch((e) => onError(String(e)));
  return (
    <div className="panel access">
      {rows.map(({ which, name, why }) => {
        const state = perms[which]!;
        const [tone, label] = PILL[state];
        return (
          <div key={which} className="row">
            <div className="row__text">
              <span className="row__label">
                {name}
                <span className={`pill ${tone}`}>{label}</span>
              </span>
              <p className="row__help">
                {why}
                {which === "screen" && state !== "allowed" && " macOS applies a change only after Helpy restarts."}
              </p>
            </div>
            <div className="row__control access__actions">
              {state === "allowed" ? (
                <button type="button" className="link-btn" onClick={() => run(() => api.permissionsOpen(which))}>
                  Open System Settings
                </button>
              ) : (
                <>
                  {which === "screen" && (
                    <button type="button" className="link-btn" onClick={() => run(api.permissionsRelaunch)}>
                      Restart Helpy
                    </button>
                  )}
                  <button type="button" className="btn btn--sm" onClick={() => run(() => api.permissionsRequest(which).then(setPerms))}>
                    {state === "notAsked" ? "Allow…" : "Turn on…"}
                  </button>
                </>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
