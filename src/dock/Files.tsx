import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import "./files.css";

const base = (p: string) => p.split(/[\\/]/).pop() ?? p;

/**
 * Files an agent made: the newest CSV previewed as a small table, and an
 * Open button for each (they open in the app that handles them).
 */
export function AgentFiles({ agent, files, compact }: { agent: string; files: string[]; compact?: boolean }) {
  const csv = [...files].reverse().find((f) => f.toLowerCase().endsWith(".csv"));
  const [rows, setRows] = useState<string[][] | null>(null);
  useEffect(() => {
    setRows(null);
    if (csv) api.csvPreview(agent, csv).then(setRows, () => setRows(null));
  }, [agent, csv]);
  if (files.length === 0) return null;
  const cols = compact ? 4 : 8;
  const shown = rows?.slice(0, compact ? 4 : 7) ?? [];

  return (
    <div className="afiles">
      {csv && shown.length > 0 && (
        <div className="afiles__table" role="table" aria-label={base(csv)}>
          {shown.map((r, i) => (
            <div key={i} className={`afiles__row${i === 0 ? " is-head" : ""}`} role="row" style={{ ["--i" as string]: i }}>
              {r.slice(0, cols).map((c, j) => (
                <span key={j} role={i === 0 ? "columnheader" : "cell"} title={c}>
                  {c}
                </span>
              ))}
            </div>
          ))}
        </div>
      )}
      <div className="afiles__list">
        {files.slice(-4).map((f) => (
          <button key={f} type="button" className="afiles__file" title={f} onClick={() => api.openAgentFile(agent, f)}>
            <svg viewBox="0 0 20 20" width="13" height="13" aria-hidden="true">
              <path d="M5 2.5h6.5L15 6v11.5H5zM11.5 2.5V6H15" />
            </svg>
            <span>{base(f)}</span>
            <span className="afiles__open">Open</span>
          </button>
        ))}
      </div>
    </div>
  );
}
