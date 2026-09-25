import { useEffect, useState } from "react";
import type { Ai } from "../../bindings/Ai";
import type { LocalServer } from "../../bindings/LocalServer";
import type { ModelConfig } from "../../bindings/ModelConfig";
import type { ModelInfo } from "../../bindings/ModelInfo";
import type { ProviderConfig } from "../../bindings/ProviderConfig";
import type { ProviderKind } from "../../bindings/ProviderKind";
import { api } from "../../lib/ipc";
import { Toggle } from "../controls";
import { isLocal, modelFromInfo, newProvider, PRESETS, presetFor } from "./presets";
import { withoutProvider, withAutoRouting } from "./routing";

type Props = {
  ai: Ai;
  /** Saves the whole AI group; resolves to an error message or null. */
  commit: (ai: Ai) => Promise<string | null>;
};

export function ProvidersEditor({ ai, commit }: Props) {
  const [adding, setAdding] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  const [local, setLocal] = useState<LocalServer[] | "searching" | null>(null);

  const add = async (p: ProviderConfig) => {
    const next = withAutoRouting({ ...ai, providers: [...ai.providers, p] });
    if (!(await commit(next))) {
      setAdding(false);
      setLocal(null);
      setOpen(p.id);
    }
  };

  const findLocal = async () => {
    setLocal("searching");
    setLocal(await api.detectLocal());
  };

  return (
    <div className="providers">
      {ai.providers.length === 0 && !adding && (
        <div className="providers__empty">
          <p>
            <strong>No AI provider yet.</strong> Add a cloud provider with an API key, or use a model running on this computer
            with Ollama or LM Studio.
          </p>
        </div>
      )}

      {ai.providers.map((p) => (
        <ProviderCard
          key={p.id}
          provider={p}
          expanded={open === p.id}
          onToggle={() => setOpen(open === p.id ? null : p.id)}
          onChange={(updated) => commit(withAutoRouting({ ...ai, providers: ai.providers.map((x) => (x.id === p.id ? updated : x)) }))}
          onRemove={async () => {
            if (!(await commit(withoutProvider(ai, p.id)))) await api.deleteKey(p.id).catch(() => {});
          }}
        />
      ))}

      {adding ? (
        <div className="add-provider">
          <div className="add-provider__head">
            <span>Choose a provider</span>
            <button type="button" className="link-btn" onClick={() => { setAdding(false); setLocal(null); }}>
              Cancel
            </button>
          </div>
          <div className="preset-grid">
            {PRESETS.map((preset) => (
              <button key={preset.kind} type="button" className="preset" onClick={() => add(newProvider(preset.kind, ai.providers))}>
                <ProviderMark kind={preset.kind} />
                <span className="preset__name">{preset.name}</span>
                <span className="preset__sub">{preset.key === "none" ? "Runs on this computer" : preset.key === "required" ? "Needs an API key" : "Any compatible server"}</span>
              </button>
            ))}
          </div>
          <div className="local-find">
            <button type="button" className="btn btn--ghost" onClick={findLocal} disabled={local === "searching"}>
              {local === "searching" ? "Looking…" : "Find models on this computer"}
            </button>
            {Array.isArray(local) && local.length === 0 && (
              <span className="muted">No Ollama or LM Studio server found on the usual ports. Start one and try again.</span>
            )}
          </div>
          {Array.isArray(local) &&
            local.map((server) => (
              <div key={server.baseUrl} className="local-server">
                <ProviderMark kind={server.kind} />
                <div className="local-server__text">
                  <strong>{presetFor(server.kind).name}</strong>
                  <span className="mono muted">{server.baseUrl}</span>
                  <span className="muted">
                    {server.models.length} {server.models.length === 1 ? "model" : "models"}
                    {server.models.length > 0 && `: ${server.models.slice(0, 4).map((m) => m.id).join(", ")}${server.models.length > 4 ? "…" : ""}`}
                  </span>
                </div>
                <button
                  type="button"
                  className="btn btn--primary"
                  onClick={() => {
                    const p = newProvider(server.kind, ai.providers, server.baseUrl);
                    add({ ...p, models: server.models.map((m) => modelFromInfo(server.kind, m)) });
                  }}
                >
                  Add
                </button>
              </div>
            ))}
        </div>
      ) : (
        <button type="button" className="btn btn--ghost add-btn" onClick={() => setAdding(true)}>
          + Add provider
        </button>
      )}
    </div>
  );
}

export function ProviderMark({ kind }: { kind: ProviderKind }) {
  return (
    <span className={`pmark pmark--${kind}`} aria-hidden="true">
      {presetFor(kind).mark}
    </span>
  );
}

type KeyState = "unknown" | "saved" | "missing" | "error";

function ProviderCard(props: {
  provider: ProviderConfig;
  expanded: boolean;
  onToggle: () => void;
  onChange: (p: ProviderConfig) => Promise<string | null>;
  onRemove: () => void;
}) {
  const { provider: p, expanded, onToggle, onChange, onRemove } = props;
  const preset = presetFor(p.kind);
  const [key, setKey] = useState<KeyState>("unknown");
  const [test, setTest] = useState<{ ok: boolean; text: string } | "testing" | null>(null);

  const refreshKey = () =>
    api.hasKey(p.id).then(
      (has) => setKey(has ? "saved" : "missing"),
      () => setKey("error"),
    );
  useEffect(() => void refreshKey(), [p.id]);

  const runTest = async () => {
    setTest("testing");
    try {
      setTest({ ok: true, text: await api.testProvider(p) });
    } catch (e) {
      setTest({ ok: false, text: String(e) });
    }
  };

  return (
    <div className={`pcard${expanded ? " is-open" : ""}`}>
      <div className="pcard__head">
        <ProviderMark kind={p.kind} />
        <div className="pcard__title">
          <strong>{p.name}</strong>
          <span className="mono muted">{p.baseUrl || "No address yet"}</span>
        </div>
        <KeyPill state={key} need={preset.key} />
        <button type="button" className="btn btn--ghost btn--sm" onClick={runTest} disabled={test === "testing"}>
          {test === "testing" ? "Testing…" : "Test"}
        </button>
        <button type="button" className="btn btn--ghost btn--sm" aria-expanded={expanded} onClick={onToggle}>
          {expanded ? "Done" : "Edit"}
        </button>
      </div>
      {!expanded && p.models.length > 0 && (
        <div className="chips">
          {p.models.map((m) => (
            <span key={m.id} className="mchip" title={m.vision ? "Can see images" : "Text only"}>
              {m.vision && <EyeIcon />}
              {m.id}
            </span>
          ))}
        </div>
      )}
      {!expanded && p.models.length === 0 && <p className="pcard__note">No models added yet. Open Edit to load them.</p>}
      {test && test !== "testing" && <p className={`pcard__test ${test.ok ? "is-ok" : "is-err"}`}>{test.text}</p>}
      {expanded && <ProviderForm provider={p} keyState={key} onKeyChanged={refreshKey} onChange={onChange} onRemove={onRemove} />}
    </div>
  );
}

function KeyPill({ state, need }: { state: KeyState; need: "required" | "optional" | "none" }) {
  if (need === "none" && state !== "saved") return <span className="pill pill--idle">No key needed</span>;
  if (state === "saved") return <span className="pill pill--ok">Key saved</span>;
  // Providers that work without a key don't need the keychain at all.
  if (state === "error" && need === "required") return <span className="pill pill--err">Keychain unavailable</span>;
  if (state === "missing" && need === "required") return <span className="pill pill--warn">Needs a key</span>;
  return null;
}

function ProviderForm(props: {
  provider: ProviderConfig;
  keyState: KeyState;
  onKeyChanged: () => void;
  onChange: (p: ProviderConfig) => Promise<string | null>;
  onRemove: () => void;
}) {
  const { provider: p, keyState, onKeyChanged, onChange, onRemove } = props;
  const preset = presetFor(p.kind);
  const [name, setName] = useState(p.name);
  const [url, setUrl] = useState(p.baseUrl);
  const [keyText, setKeyText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState<ModelInfo[] | "loading" | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [manual, setManual] = useState("");
  const [confirmRemove, setConfirmRemove] = useState(false);

  useEffect(() => setName(p.name), [p.name]);
  useEffect(() => setUrl(p.baseUrl), [p.baseUrl]);

  const save = async (next: ProviderConfig) => setError(await onChange(next));
  const setModel = (id: string, patch: Partial<ModelConfig>) =>
    save({ ...p, models: p.models.map((m) => (m.id === id ? { ...m, ...patch } : m)) });

  const saveKey = async () => {
    try {
      await api.setKey(p.id, keyText);
      setKeyText("");
      setError(null);
      onKeyChanged();
    } catch (e) {
      setError(String(e));
    }
  };

  const load = async () => {
    setLoaded("loading");
    try {
      const models = await api.listModels({ ...p, baseUrl: url });
      setLoaded(models);
      setPicked(new Set());
      setError(null);
    } catch (e) {
      setLoaded(null);
      setError(String(e));
    }
  };

  const available = Array.isArray(loaded) ? loaded.filter((m) => !p.models.some((x) => x.id === m.id)) : [];

  return (
    <div className="pform">
      <div className="pform__grid">
        <label className="field">
          <span>Name</span>
          <input id={`p-${p.id}-name`} value={name} onChange={(e) => setName(e.target.value)} onBlur={() => name !== p.name && save({ ...p, name })} />
        </label>
        <label className="field">
          <span>Address</span>
          <input
            id={`p-${p.id}-url`}
            className="mono"
            value={url}
            placeholder="https://example.com/v1"
            onChange={(e) => setUrl(e.target.value)}
            onBlur={() => url !== p.baseUrl && save({ ...p, baseUrl: url.trim() })}
          />
        </label>
      </div>

      {preset.key !== "none" && (
        <div className="field">
          <span>API key {preset.key === "optional" && <em className="muted">(optional)</em>}</span>
          <div className="keyrow">
            <input
              id={`p-${p.id}-key`}
              type="password"
              autoComplete="off"
              className="mono"
              placeholder={keyState === "saved" ? "•••••••• saved in your keychain" : "Paste your key"}
              value={keyText}
              onChange={(e) => setKeyText(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && keyText && saveKey()}
            />
            <button type="button" className="btn btn--primary" disabled={!keyText} onClick={saveKey}>
              Save key
            </button>
            {keyState === "saved" && (
              <button type="button" className="btn btn--ghost" onClick={() => api.deleteKey(p.id).then(onKeyChanged, (e) => setError(String(e)))}>
                Remove
              </button>
            )}
          </div>
          {preset.keyHint && keyState !== "saved" && <span className="muted">{preset.keyHint}</span>}
        </div>
      )}

      <div className="field">
        <span>Models</span>
        {p.models.length > 0 && (
          <div className="mtable" role="table" aria-label={`${p.name} models`}>
            <div className="mtable__row mtable__row--head" role="row">
              <span role="columnheader">Model</span>
              <span role="columnheader" title="Can read screenshots">Sees images</span>
              <span role="columnheader" title="Calls tools reliably">Uses tools</span>
              <span role="columnheader" title="US dollars per million input tokens">$ in / M</span>
              <span role="columnheader" title="US dollars per million output tokens">$ out / M</span>
              <span />
            </div>
            {p.models.map((m) => (
              <div key={m.id} className="mtable__row" role="row">
                <span className="mono mtable__id" role="cell">{m.id}</span>
                <span role="cell"><Toggle id={`m-${p.id}-${m.id}-vision`} checked={m.vision} onChange={(v) => setModel(m.id, { vision: v })} /></span>
                <span role="cell"><Toggle id={`m-${p.id}-${m.id}-tools`} checked={m.tools} onChange={(v) => setModel(m.id, { tools: v })} /></span>
                <span role="cell"><PriceInput value={m.inputPrice} onCommit={(v) => setModel(m.id, { inputPrice: v })} /></span>
                <span role="cell"><PriceInput value={m.outputPrice} onCommit={(v) => setModel(m.id, { outputPrice: v })} /></span>
                <button type="button" className="icon-btn" aria-label={`Remove ${m.id}`} onClick={() => save({ ...p, models: p.models.filter((x) => x.id !== m.id) })}>
                  ×
                </button>
              </div>
            ))}
          </div>
        )}
        <div className="model-add">
          <button type="button" className="btn btn--ghost" onClick={load} disabled={loaded === "loading"}>
            {loaded === "loading" ? "Loading…" : "Load models from provider"}
          </button>
          <span className="muted">or</span>
          <input
            id={`p-${p.id}-manual`}
            className="mono"
            placeholder="model id"
            value={manual}
            onChange={(e) => setManual(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && manual.trim()) {
                save({ ...p, models: [...p.models, modelFromInfo(p.kind, { id: manual.trim(), vision: null, tools: null })] });
                setManual("");
              }
            }}
          />
        </div>
        {Array.isArray(loaded) && (
          <div className="picker">
            {available.length === 0 ? (
              <span className="muted">Every model this provider offers is already added.</span>
            ) : (
              <>
                <div className="picker__list">
                  {available.map((m) => (
                    <label key={m.id} className="picker__item">
                      <input
                        type="checkbox"
                        checked={picked.has(m.id)}
                        onChange={(e) => {
                          const next = new Set(picked);
                          if (e.target.checked) next.add(m.id);
                          else next.delete(m.id);
                          setPicked(next);
                        }}
                      />
                      <span className="mono">{m.id}</span>
                      {m.vision && <span className="tag"><EyeIcon /> images</span>}
                    </label>
                  ))}
                </div>
                <button
                  type="button"
                  className="btn btn--primary"
                  disabled={picked.size === 0}
                  onClick={() => {
                    save({ ...p, models: [...p.models, ...available.filter((m) => picked.has(m.id)).map((m) => modelFromInfo(p.kind, m))] });
                    setLoaded(null);
                  }}
                >
                  Add {picked.size || ""} {picked.size === 1 ? "model" : "models"}
                </button>
              </>
            )}
          </div>
        )}
        {isLocal(p.kind) && <span className="muted">Local models are free, so their prices are set to 0.</span>}
      </div>

      {error && <p className="row__msg row__msg--err">{error}</p>}

      <div className="pform__foot">
        {confirmRemove ? (
          <div className="confirm">
            <span>Remove {p.name} and its key?</span>
            <button type="button" className="btn btn--danger" onClick={onRemove}>Remove</button>
            <button type="button" className="btn btn--ghost" onClick={() => setConfirmRemove(false)}>Cancel</button>
          </div>
        ) : (
          <button type="button" className="link-btn link-btn--danger" onClick={() => setConfirmRemove(true)}>
            Remove provider
          </button>
        )}
      </div>
    </div>
  );
}

function PriceInput({ value, onCommit }: { value: number | null; onCommit: (v: number | null) => void }) {
  const [text, setText] = useState(value === null ? "" : String(value));
  useEffect(() => setText(value === null ? "" : String(value)), [value]);
  const commit = () => {
    const t = text.trim();
    const v = t === "" ? null : Number(t);
    if (v === null || (Number.isFinite(v) && v >= 0)) {
      if (v !== value) onCommit(v);
    } else {
      setText(value === null ? "" : String(value));
    }
  };
  return (
    <input
      className="price mono"
      inputMode="decimal"
      placeholder="?"
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}

export function EyeIcon() {
  return (
    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true" className="eye">
      <path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8Z" />
      <circle cx="8" cy="8" r="2" />
    </svg>
  );
}
