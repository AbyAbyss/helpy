import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Plan } from "../bindings/Plan";
import type { RunMode } from "../bindings/RunMode";
import { api, EVENTS } from "../lib/ipc";

/**
 * The plan card: what Helpy is about to start, and the only way it starts.
 * Enter or "yes, go" starts it; Esc or "cancel" drops it.
 */
export function PlanCard() {
  const [plan, setPlan] = useState<Plan | null>(null);
  const [mode, setMode] = useState<RunMode>("single");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const take = (p: Plan | null) => {
      setPlan(p);
      setError(null);
      setBusy(false);
      if (p) setMode(p.mode);
    };
    api.planCurrent().then(take);
    const off = listen<Plan | null>(EVENTS.agentPlan, (e) => take(e.payload));
    return () => void off.then((f) => f());
  }, []);

  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const win = getCurrentWindow();
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect();
      win.setSize(new LogicalSize(Math.ceil(r.width), Math.ceil(r.height))).catch(() => {});
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const start = async () => {
    if (!plan || busy) return;
    setBusy(true);
    try {
      await api.planStart(plan.id, mode);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") api.planCancel();
      if (e.key === "Enter" && !e.shiftKey) start();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const label = (id: string) => plan?.groups.find((g) => g.id === id)?.label ?? id;
  const many = (plan?.agents.length ?? 0) > 1;

  return (
    <div ref={root} className="plan">
      {plan && (
        <section className="pcard" aria-label="Agents plan">
          <header className="pcard__head">
            <svg viewBox="0 0 64 64" width="20" height="20" aria-hidden="true">
              <path d="M6 6 L26 13.5 A22 22 0 1 1 13.5 26 Z" fill="var(--accent)" />
              <ellipse cx="29" cy="33" rx="3.2" ry="4.4" fill="#141821" />
              <ellipse cx="41" cy="33" rx="3.2" ry="4.4" fill="#141821" />
            </svg>
            <div>
              <h1>{many ? `${plan.agents.length} agents` : "1 agent"} ready to start</h1>
              <p className="pcard__request" title={plan.request}>
                “{plan.request}”
              </p>
            </div>
          </header>

          {many && (
            <div className="seg" role="radiogroup" aria-label="Run mode">
              {(["parallel", "sequential"] as const).map((m) => (
                <button key={m} type="button" role="radio" aria-checked={mode === m} onClick={() => setMode(m)}>
                  {m === "parallel" ? "All at once" : "One after another"}
                </button>
              ))}
            </div>
          )}

          <ol className={`plist${many && mode === "sequential" ? " plist--seq" : ""}`}>
            {plan.agents.map((a, i) => (
              <li key={i} className="pagent">
                <span className="pagent__num" aria-hidden="true">
                  {many ? i + 1 : ""}
                </span>
                <div className="pagent__body">
                  <div className="pagent__top">
                    <strong>{a.name}</strong>
                    {a.keepOpen && <span className="tag tag--open">Stays open for changes</span>}
                  </div>
                  <p className="pagent__goal">{a.goal}</p>
                  {a.tools.length > 0 && (
                    <div className="pagent__tools">
                      {a.tools.map((t) => (
                        <span key={t} className="tag">
                          {label(t)}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
              </li>
            ))}
          </ol>

          {(plan.asks.length > 0 || plan.newFolders.length > 0 || plan.hasImage) && (
            <ul className="pnotes">
              {plan.asks.length > 0 && (
                <li>
                  <Dot tone="ask" />
                  Asks you first before {joinList(plan.asks)}.
                </li>
              )}
              {plan.newFolders.map((f) => (
                <li key={f}>
                  <Dot tone="folder" />
                  <span>
                    Needs your <b>{shortPath(f)}</b> folder. Starting lets agents use it; every change can be undone.
                  </span>
                </li>
              ))}
              {plan.hasImage && (
                <li>
                  <Dot tone="image" />
                  Takes along a picture of what was on your screen.
                </li>
              )}
            </ul>
          )}

          {error && (
            <p className="pcard__err" role="alert">
              {error}
            </p>
          )}

          <footer className="pcard__foot">
            <span className="pcard__hint">Or say “yes, go”</span>
            <button type="button" className="btn btn--quiet" onClick={() => api.planCancel()}>
              Cancel <kbd>Esc</kbd>
            </button>
            <button type="button" className="btn btn--primary" onClick={start} disabled={busy} autoFocus>
              Start <kbd>↵</kbd>
            </button>
          </footer>
        </section>
      )}
    </div>
  );
}

function Dot({ tone }: { tone: "ask" | "folder" | "image" }) {
  return <span className={`dot dot--${tone}`} aria-hidden="true" />;
}

function joinList(items: string[]) {
  return items.length < 2 ? items.join("") : `${items.slice(0, -1).join(", ")} or ${items[items.length - 1]}`;
}

/** "/home/me/Desktop" → "~/Desktop" where it's clearly the home folder. */
function shortPath(p: string) {
  return p.replace(/^\/(home|Users)\/[^/]+/, "~").replace(/^[A-Z]:\\Users\\[^\\]+/, "~");
}
