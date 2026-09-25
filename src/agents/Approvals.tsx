import { useState } from "react";
import type { AgentView } from "../bindings/AgentView";
import type { Pending } from "../bindings/Pending";
import { api } from "../lib/ipc";

type Approval = Extract<Pending, { type: "approval" }>;

/** Fields shown as a big box rather than one line. */
const LONG = ["body", "text", "description", "content", "message", "comment"];

/**
 * One action waiting for the user's OK: everything it will do, with its
 * text fields editable before approving.
 */
export function ApprovalBox({ agent, pending: p, showAgent }: { agent: AgentView; pending: Approval; showAgent?: boolean }) {
  const [edits, setEdits] = useState<Record<string, string>>({});
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const edited = Object.keys(edits).some((k) => edits[k] !== String(p.args[k] ?? ""));

  const answer = async (approve: boolean) => {
    setError(null);
    try {
      if (!approve) await api.agentAnswer(agent.id, { type: "reject", note: note.trim() || null });
      else if (edited) await api.agentAnswer(agent.id, { type: "edit", args: { ...p.args, ...edits } });
      else await api.agentAnswer(agent.id, { type: "approve" });
    } catch (e) {
      setError(String(e));
    }
  };

  const fields = p.editable.filter((k) => typeof p.args[k] === "string" || p.args[k] == null);
  return (
    <section className="box box--alert approval">
      <header className="approval__head">
        <span className="tag">{p.source}</span>
        {showAgent && (
          <button type="button" className="approval__agent" onClick={() => api.openAgentPanel(agent.id)}>
            {agent.name}
          </button>
        )}
      </header>
      <p className="box__summary">{p.summary}</p>
      {fields.length > 0 ? (
        <div className="approval__fields">
          {fields.map((k) => {
            const value = edits[k] ?? String(p.args[k] ?? "");
            const set = (v: string) => setEdits((e) => ({ ...e, [k]: v }));
            return (
              <label key={k} className="approval__field">
                <span>{k.replace(/_/g, " ")}</span>
                {LONG.includes(k) || value.includes("\n") ? (
                  <textarea className="field field--area" rows={Math.min(12, Math.max(3, value.split("\n").length + 1))} value={value} onChange={(e) => set(e.target.value)} />
                ) : (
                  <input className="field" value={value} onChange={(e) => set(e.target.value)} />
                )}
              </label>
            );
          })}
        </div>
      ) : (
        p.detail && <pre className="box__detail">{p.detail}</pre>
      )}
      <input className="field" value={note} onChange={(e) => setNote(e.target.value)} placeholder="Why not? (optional, if you reject)" />
      {error && <p className="err-text">{error}</p>}
      <div className="box__buttons">
        <button type="button" className="btn btn--primary" onClick={() => answer(true)}>
          {edited ? "Approve with changes" : "Approve"}
        </button>
        <button type="button" className="btn" onClick={() => answer(false)}>
          Reject
        </button>
      </div>
    </section>
  );
}

/** Every action waiting for the user's OK, across all agents. */
export function Inbox({ agents }: { agents: AgentView[] }) {
  const waiting = agents
    .filter((a): a is AgentView & { pending: Approval } => a.pending?.type === "approval")
    .sort((a, b) => a.order - b.order);
  const bySource = new Map<string, AgentView[]>();
  for (const a of waiting) bySource.set(a.pending.source, [...(bySource.get(a.pending.source) ?? []), a]);
  const approveAll = (list: AgentView[]) => Promise.allSettled(list.map((a) => api.agentAnswer(a.id, { type: "approve" })));

  return (
    <article className="detail inbox">
      <header className="detail__head">
        <div className="detail__title">
          <h1>Approval inbox</h1>
          {waiting.length > 0 && <span className="pill">{waiting.length} waiting</span>}
        </div>
        <div className="detail__actions">
          {[...bySource.entries()]
            .filter(([, list]) => list.length > 1)
            .map(([source, list]) => (
              <button key={source} type="button" className="btn" onClick={() => approveAll(list)}>
                Approve all {list.length} from {source}
              </button>
            ))}
        </div>
      </header>
      {waiting.length === 0 ? (
        <p className="muted">Nothing is waiting for your OK. When an agent wants to send, post or change something, it shows up here.</p>
      ) : (
        <>
          <p className="muted small">Or say “approve”, “reject”, or “approve all from Gmail”.</p>
          {waiting.map((a) => (
            <ApprovalBox key={a.pending.id} agent={a} pending={a.pending} showAgent />
          ))}
        </>
      )}
    </article>
  );
}
