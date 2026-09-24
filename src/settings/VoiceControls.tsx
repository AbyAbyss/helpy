import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { DownloadProgress } from "../bindings/DownloadProgress";
import type { PiperVoice } from "../bindings/PiperVoice";
import type { Settings } from "../bindings/Settings";
import type { SystemVoice } from "../bindings/SystemVoice";
import type { WhisperModel } from "../bindings/WhisperModel";
import { api, EVENTS } from "../lib/ipc";

export function formatSize(bytes: number | null): string {
  if (bytes === null) return "size unknown";
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  return `${Math.round(bytes / 1e6)} MB`;
}

/** Download progress per item ("whisper:base"), from the download events. */
function useDownloads() {
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  useEffect(() => {
    const off = listen<DownloadProgress>(EVENTS.download, (e) => setProgress((p) => ({ ...p, [e.payload.item]: e.payload })));
    return () => void off.then((f) => f());
  }, []);
  return progress;
}

function Progress({ p }: { p?: DownloadProgress }) {
  const share = p?.total ? p.done / p.total : null;
  return (
    <span className="dl">
      <span className="dl__bar">
        <span style={{ width: share === null ? "30%" : `${share * 100}%` }} className={share === null ? "is-indeterminate" : ""} />
      </span>
      <span className="mono dl__text">{share === null ? "Starting…" : `${Math.round(share * 100)}%`}</span>
    </span>
  );
}

export function WhisperModels({ value, onChange }: { value: string; onChange: (name: string) => void }) {
  const [models, setModels] = useState<WhisperModel[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const progress = useDownloads();
  const refresh = () => api.whisperModels().then(setModels);
  useEffect(() => void refresh(), []);

  const download = async (name: string) => {
    setBusy(name);
    setError(null);
    try {
      await api.whisperDownload(name);
      onChange(name);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
      refresh();
    }
  };

  if (!models) return <p className="muted">Checking models…</p>;
  return (
    <div className="models">
      {models.map((m) => (
        <div key={m.name} className={`model${m.name === value ? " is-current" : ""}`}>
          <div className="model__text">
            <span className="mono model__name">{m.name}</span>
            <span className="muted">{m.description}</span>
          </div>
          <span className="mono model__size">{formatSize(m.size)}</span>
          <div className="model__actions">
            {busy === m.name ? (
              <Progress p={progress[`whisper:${m.name}`]} />
            ) : m.installed ? (
              <>
                {m.name === value ? (
                  <span className="pill pill--ok">In use</span>
                ) : (
                  <button type="button" className="btn btn--ghost btn--sm" onClick={() => onChange(m.name)}>
                    Use
                  </button>
                )}
                <button
                  type="button"
                  className="icon-btn"
                  aria-label={`Delete ${m.name}`}
                  title="Delete from this computer"
                  onClick={() => api.whisperDelete(m.name).then(refresh, (e) => setError(String(e)))}
                >
                  ×
                </button>
              </>
            ) : (
              <button type="button" className="btn btn--ghost btn--sm" disabled={busy !== null} onClick={() => download(m.name)}>
                Download
              </button>
            )}
          </div>
        </div>
      ))}
      {error && <p className="row__msg row__msg--err">{error}</p>}
    </div>
  );
}

/** Device picker with a live level meter while the page is open. */
export function MicPicker({ value, denoise, onChange }: { value: string | null; denoise: boolean; onChange: (d: string | null) => void }) {
  const [devices, setDevices] = useState<string[]>([]);
  const [level, setLevel] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => void api.inputDevices().then(setDevices), []);
  useEffect(() => {
    setError(null);
    api.meterStart(value, denoise).catch((e) => setError(String(e)));
    const off = listen<number>(EVENTS.voiceMeter, (e) => setLevel(e.payload));
    return () => {
      api.meterStop();
      void off.then((f) => f());
    };
  }, [value, denoise]);

  return (
    <div className="mic">
      <div className="select">
        <select id="setting-voiceInput-microphone" value={value ?? ""} onChange={(e) => onChange(e.target.value || null)}>
          <option value="">System default</option>
          {devices.map((d) => (
            <option key={d} value={d}>
              {d}
            </option>
          ))}
        </select>
        <svg className="select__chevron" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M3 4.5 6 7.5 9 4.5" />
        </svg>
      </div>
      {error ? (
        <span className="mic__err">{error}</span>
      ) : (
        <div className="level" role="meter" aria-label="Microphone level" aria-valuenow={Math.round(level * 100)} aria-valuemin={0} aria-valuemax={100}>
          {Array.from({ length: 16 }, (_, i) => (
            <span key={i} className={i / 16 < level ? (i >= 13 ? "is-on is-hot" : "is-on") : ""} />
          ))}
        </div>
      )}
    </div>
  );
}

/** OpenAI or OpenAI-compatible providers, whose key speech services borrow. */
export function ProviderSelect({ id, settings, value, onChange }: { id: string; settings: Settings; value: string | null; onChange: (v: string | null) => void }) {
  const providers = settings.ai.providers.filter((p) => p.kind === "openAi" || p.kind === "openAiCompatible");
  if (providers.length === 0) return <span className="muted">Add an OpenAI provider under AI providers first.</span>;
  return (
    <div className="select">
      <select id={id} value={value ?? ""} onChange={(e) => onChange(e.target.value || null)}>
        <option value="">Choose…</option>
        {providers.map((p) => (
          <option key={p.id} value={p.id}>
            {p.name}
          </option>
        ))}
      </select>
      <svg className="select__chevron" viewBox="0 0 12 12" aria-hidden="true">
        <path d="M3 4.5 6 7.5 9 4.5" />
      </svg>
    </div>
  );
}

export function TextInput({ id, value, placeholder, invalid, onCommit }: { id: string; value: string; placeholder: string; invalid: boolean; onCommit: (v: string) => void }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  const commit = () => text.trim() !== value && onCommit(text.trim());
  return (
    <input
      id={id}
      className={`text-input${invalid ? " is-invalid" : ""}`}
      value={text}
      placeholder={placeholder}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}

export function DeepgramField({ model, invalid, onModel }: { model: string; invalid: boolean; onModel: (v: string) => void }) {
  const [has, setHas] = useState<boolean | null>(null);
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const refresh = () => api.hasDeepgramKey().then(setHas, (e) => setError(String(e)));
  useEffect(() => void refresh(), []);
  return (
    <div className="stack">
      <div className="keyrow">
        <input
          id="deepgram-key"
          type="password"
          autoComplete="off"
          className="text-input mono"
          placeholder={has ? "•••••••• saved in your keychain" : "Deepgram API key"}
          value={key}
          onChange={(e) => setKey(e.target.value)}
        />
        <button
          type="button"
          className="btn btn--primary"
          disabled={!key}
          onClick={() =>
            api.setDeepgramKey(key).then(
              () => {
                setKey("");
                refresh();
              },
              (e) => setError(String(e)),
            )
          }
        >
          Save key
        </button>
      </div>
      <label className="inline-field">
        <span className="muted">Model</span>
        <TextInput id="setting-voiceInput-deepgramModel" value={model} placeholder="nova-3" invalid={invalid} onCommit={onModel} />
      </label>
      {error && <span className="row__msg row__msg--err">{error}</span>}
    </div>
  );
}

export function SystemVoiceSelect({ id, value, onChange }: { id: string; value: string | null; onChange: (v: string | null) => void }) {
  const [voices, setVoices] = useState<SystemVoice[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => void api.systemVoices().then(setVoices, (e) => setError(String(e))), []);
  if (error) return <span className="mic__err">{error}</span>;
  return (
    <div className="select">
      <select id={id} value={value ?? ""} onChange={(e) => onChange(e.target.value || null)} disabled={!voices}>
        <option value="">System default</option>
        {voices?.map((v) => (
          <option key={v.id} value={v.id}>
            {v.name} ({v.language})
          </option>
        ))}
      </select>
      <svg className="select__chevron" viewBox="0 0 12 12" aria-hidden="true">
        <path d="M3 4.5 6 7.5 9 4.5" />
      </svg>
    </div>
  );
}

/** Browse the Piper catalogue by language; download and pick a voice. */
export function PiperVoicePicker({ value, onChange }: { value: string; onChange: (key: string) => void }) {
  const [voices, setVoices] = useState<PiperVoice[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [language, setLanguage] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const progress = useDownloads();
  const refresh = () => api.piperVoices().then(setVoices, (e) => setError(String(e)));
  useEffect(() => void refresh(), []);

  const languages = useMemo(() => [...new Set(voices?.map((v) => v.language))], [voices]);
  const current = voices?.find((v) => v.key === value);
  const shownLanguage = language ?? current?.language ?? languages.find((l) => l.startsWith("English (United States)")) ?? languages[0];

  if (error) return <span className="mic__err">{error}</span>;
  if (!voices) return <p className="muted">Loading voices…</p>;

  const install = async (key: string) => {
    setBusy(key);
    setError(null);
    try {
      await api.piperInstall(key);
      onChange(key);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
      refresh();
    }
  };

  return (
    <div className="models">
      <div className="select">
        <select id="piper-language" value={shownLanguage} onChange={(e) => setLanguage(e.target.value)}>
          {languages.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
        <svg className="select__chevron" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M3 4.5 6 7.5 9 4.5" />
        </svg>
      </div>
      {voices
        .filter((v) => v.language === shownLanguage)
        .map((v) => (
          <div key={v.key} className={`model${v.key === value ? " is-current" : ""}`}>
            <div className="model__text">
              <span className="model__name">{v.name[0].toUpperCase() + v.name.slice(1).replace(/_/g, " ")}</span>
              <span className="muted">{v.quality} quality</span>
            </div>
            <span className="mono model__size">{formatSize(v.size)}</span>
            <div className="model__actions">
              {busy === v.key ? (
                <Progress p={progress[`piper:${v.key}`] ?? progress["piper:engine"]} />
              ) : v.installed ? (
                v.key === value ? (
                  <span className="pill pill--ok">In use</span>
                ) : (
                  <button type="button" className="btn btn--ghost btn--sm" onClick={() => onChange(v.key)}>
                    Use
                  </button>
                )
              ) : (
                <button type="button" className="btn btn--ghost btn--sm" disabled={busy !== null} onClick={() => install(v.key)}>
                  Download
                </button>
              )}
            </div>
          </div>
        ))}
    </div>
  );
}

/** Play sample, plus the latest speech error so a silent failure is visible. */
export function SampleButton() {
  const [speaking, setSpeaking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const offs = [
      listen<boolean>(EVENTS.speaking, (e) => setSpeaking(e.payload)),
      listen<string>(EVENTS.speakError, (e) => setError(e.payload)),
    ];
    return () => offs.forEach((o) => void o.then((f) => f()));
  }, []);
  return (
    <div className="sample">
      <button
        type="button"
        className="btn btn--ghost"
        onClick={() => {
          setError(null);
          if (speaking) {
            api.stopSpeaking();
          } else {
            api.playSample();
          }
        }}
      >
        {speaking ? "Stop" : "▶ Play sample"}
      </button>
      {error && <span className="mic__err">{error}</span>}
    </div>
  );
}
