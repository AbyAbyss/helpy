import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { ConnectorConfig } from "../bindings/ConnectorConfig";
import type { ConnectorInfo } from "../bindings/ConnectorInfo";
import type { KeyValue } from "../bindings/KeyValue";
import type { McpServer } from "../bindings/McpServer";
import type { OAuthApp } from "../bindings/OAuthApp";
import type { Permission } from "../bindings/Permission";
import type { Preset } from "../bindings/Preset";
import type { Rule } from "../bindings/Rule";
import type { ServerStatus } from "../bindings/ServerStatus";
import { api, EVENTS } from "../lib/ipc";
import { Segmented, Toggle } from "./controls";

const RULES = [
  { value: "allow", label: "Allow" },
  { value: "ask", label: "Ask me" },
  { value: "never", label: "Never" },
];

const ACCESS = [
  { value: "readOnly", label: "Read only" },
  { value: "readWrite", label: "Read and write" },
];

/** Built-in connectors, refreshed when a connection changes. */
function useConnectors() {
  const [list, setList] = useState<ConnectorInfo[]>([]);
  useEffect(() => {
    const load = () => void api.connectors().then(setList, () => {});
    load();
    const off = listen(EVENTS.connectorsChanged, load);
    return () => void off.then((f) => f());
  }, []);
  return list;
}

function errorText(e: unknown) {
  return String(e).replace(/^Error: /, "");
}

function when(ms: number | null) {
  if (!ms) return "never";
  const d = new Date(ms);
  return d.toDateString() === new Date().toDateString()
    ? d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : d.toLocaleDateString([], { month: "short", day: "numeric" });
}

/** "search the user's email" (written for the planner) as "Search your email". */
function forYou(about: string) {
  const t = about.replace(/\bthe user's\b/g, "your").replace(/\bas the user\b/g, "as you");
  return t[0].toUpperCase() + t.slice(1) + ".";
}

function Mark({ text }: { text: string }) {
  return (
    <span className="pmark" aria-hidden="true">
      {text}
    </span>
  );
}

// ---------- Built-in services ----------

export function ConnectorList({ value, onChange }: { value: Record<string, ConnectorConfig | undefined>; onChange: (v: Record<string, ConnectorConfig | undefined>) => void }) {
  const list = useConnectors();
  const [open, setOpen] = useState<string | null>(null);
  return (
    <div className="providers">
      {list.map((c) => (
        <ConnectorCard
          key={c.id}
          info={c}
          config={value[c.id] ?? { permission: "readOnly", rules: {} }}
          expanded={open === c.id}
          onToggle={() => setOpen(open === c.id ? null : c.id)}
          onChange={(cfg) => onChange({ ...value, [c.id]: cfg })}
        />
      ))}
    </div>
  );
}

function ConnectorCard(props: { info: ConnectorInfo; config: ConnectorConfig; expanded: boolean; onToggle: () => void; onChange: (c: ConnectorConfig) => void }) {
  const { info: c, config, expanded, onToggle, onChange } = props;
  const [busy, setBusy] = useState<"connect" | "test" | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const conn = c.connection;

  const run = async (kind: "connect" | "test", f: () => Promise<string>) => {
    setBusy(kind);
    setMessage(null);
    try {
      setMessage({ ok: true, text: await f() });
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    }
    setBusy(null);
  };
  const connect = () => run("connect", async () => `Connected as ${(await api.connectorConnect(c.id)).account}.`);
  const needsReconnect = conn && config.permission === "readWrite" && conn.granted === "readOnly";

  return (
    <div className={`pcard${expanded ? " is-open" : ""}`}>
      <div className="pcard__head">
        <Mark text={c.name.split(/\s+/).map((w) => w[0]).join("").slice(0, 2)} />
        <div className="pcard__title">
          <strong>{c.name}</strong>
          <span className="muted">{conn ? `${conn.account} · used ${when(conn.lastUsed)}` : forYou(c.about)}</span>
        </div>
        {conn ? (
          conn.problem ? <span className="pill pill--err">Reconnect</span> : <span className="pill pill--ok">Connected</span>
        ) : (
          <span className="pill pill--idle">Not connected</span>
        )}
        {conn ? (
          <button type="button" className="btn btn--ghost btn--sm" onClick={() => run("test", () => api.connectorTest(c.id))} disabled={!!busy}>
            {busy === "test" ? "Testing…" : "Test"}
          </button>
        ) : (
          c.provider && (
            <button type="button" className="btn btn--primary btn--sm" onClick={connect} disabled={!!busy}>
              {busy === "connect" ? "Waiting for the browser…" : "Connect"}
            </button>
          )
        )}
        <button type="button" className="btn btn--ghost btn--sm" aria-expanded={expanded} onClick={onToggle}>
          {expanded ? "Done" : "Edit"}
        </button>
      </div>
      {conn?.problem && <p className="pcard__test is-err">{conn.problem}</p>}
      {message && <p className={`pcard__test ${message.ok ? "is-ok" : "is-err"}`}>{message.text}</p>}
      {expanded && (
        <div className="pform">
          <div className="crow">
            <div>
              <div className="rules__label">Agents may</div>
              <div className="rules__help">
                {needsReconnect
                  ? "It was connected read-only. Reconnect to give Helpy write access."
                  : "Read only hides every action that sends, posts or changes something."}
              </div>
            </div>
            <Segmented id={`perm-${c.id}`} value={config.permission} options={ACCESS} onChange={(v) => onChange({ ...config, permission: v as Permission })} />
          </div>
          <div className="rules">
            {c.actions.map((a) => {
              const off = a.write && config.permission === "readOnly";
              return (
                <div key={a.name} className={`rules__row${off ? " is-off" : ""}`}>
                  <div>
                    <div className="rules__label">{a.description.split(". ")[0].replace(/\.$/, "")}</div>
                    <div className="rules__help">{off ? "Needs read and write" : a.write ? "Changes something" : "Only reads"}</div>
                  </div>
                  <Segmented
                    id={`rule-${c.id}-${a.name}`}
                    value={config.rules[a.name] ?? a.rule}
                    options={RULES}
                    onChange={(v) => onChange({ ...config, rules: { ...config.rules, [a.name]: v as Rule } })}
                  />
                </div>
              );
            })}
          </div>
          {c.token && !conn && <TokenForm info={c} />}
          <div className="pform__foot">
            {conn && (
              <>
                {(needsReconnect || conn.problem) && c.provider && (
                  <button type="button" className="btn btn--primary" onClick={connect} disabled={!!busy}>
                    {busy === "connect" ? "Waiting for the browser…" : "Reconnect"}
                  </button>
                )}
                <button type="button" className="btn btn--danger" onClick={() => api.connectorDisconnect(c.id).catch(() => {})}>
                  Disconnect
                </button>
              </>
            )}
            {!conn && c.provider && <span className="muted">Signing in uses your {c.provider[0].toUpperCase() + c.provider.slice(1)} OAuth app, set up below.</span>}
          </div>
        </div>
      )}
    </div>
  );
}

function TokenForm({ info: c }: { info: ConnectorInfo }) {
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);
  const t = c.token!;
  const save = async () => {
    setError(null);
    try {
      await api.connectorSetToken(c.id, token);
      setToken("");
    } catch (e) {
      setError(errorText(e));
    }
  };
  return (
    <div className="tokenform">
      <div className="rules__label">{c.provider ? "Or paste a token" : "Connect with a token"}</div>
      <p className="rules__help">
        {t.help}{" "}
        <button type="button" className="link-btn" onClick={() => api.openLink(t.url)}>
          Open the page
        </button>
      </p>
      <div className="model-add">
        <input type="password" value={token} placeholder={t.label} onChange={(e) => setToken(e.target.value)} onKeyDown={(e) => e.key === "Enter" && save()} />
        <button type="button" className="btn btn--primary" onClick={save} disabled={!token.trim()}>
          Connect
        </button>
      </div>
      {error && <p className="pcard__test is-err">{error}</p>}
    </div>
  );
}

// ---------- OAuth apps ----------

type Guide = { name: string; url: string; steps: (redirect: string, scopes: string[]) => string[]; secret: "required" | "optional" | "none" };

/** How to make each provider's OAuth app. */
const GUIDES: Record<string, Guide> = {
  google: {
    name: "Google",
    url: "https://console.cloud.google.com/apis/credentials",
    secret: "required",
    steps: () => [
      "In Google Cloud, create a project and enable the Gmail, Google Calendar and Google Drive APIs.",
      "Under OAuth consent screen, add yourself as a test user.",
      "Under Credentials, create an OAuth client ID of type Desktop app. It needs no redirect address.",
      "Paste its client ID and client secret here.",
    ],
  },
  microsoft: {
    name: "Microsoft",
    url: "https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade",
    secret: "none",
    steps: (redirect) => [
      "In App registrations, register a new app for accounts in any organization and personal Microsoft accounts.",
      `Add a redirect under Mobile and desktop applications: ${redirect}`,
      "Paste its Application (client) ID here. No secret is needed.",
    ],
  },
  notion: {
    name: "Notion",
    url: "https://www.notion.so/profile/integrations",
    secret: "required",
    steps: (redirect) => [
      "Create a new integration and make it public.",
      `Add this redirect address: ${redirect}`,
      "Paste its OAuth client ID and client secret here. For just yourself, pasting an internal integration's token under Notion is quicker.",
    ],
  },
  slack: {
    name: "Slack",
    url: "https://api.slack.com/apps",
    secret: "optional",
    steps: (redirect, scopes) => [
      "Create an app from scratch in your workspace.",
      `Under OAuth & Permissions, add the redirect address ${redirect}, turn on PKCE, and add these user token scopes: ${scopes.join(", ")}.`,
      "Paste its client ID here. With PKCE on, no secret is needed.",
    ],
  },
  github: {
    name: "GitHub",
    url: "https://github.com/settings/developers",
    secret: "required",
    steps: (redirect) => [
      `Register a new OAuth app with the callback URL ${redirect}`,
      "Paste its client ID, and a client secret you generate, here. For just yourself, a personal access token under GitHub is quicker.",
    ],
  },
};

export function OAuthApps({ value, onChange }: { value: Record<string, OAuthApp | undefined>; onChange: (v: Record<string, OAuthApp | undefined>) => void }) {
  const list = useConnectors();
  const [open, setOpen] = useState<string | null>(null);
  const providers = [...new Set(list.map((c) => c.provider).filter((p): p is string => !!p))];
  return (
    <div className="providers">
      {providers.map((p) => {
        const uses = list.filter((c) => c.provider === p);
        const g = GUIDES[p];
        if (!g) return null;
        const clientId = value[p]?.clientId ?? "";
        const hasSecret = uses.some((c) => c.hasSecret);
        const ready = !!clientId && (g.secret !== "required" || hasSecret);
        return (
          <div key={p} className={`pcard${open === p ? " is-open" : ""}`}>
            <div className="pcard__head">
              <Mark text={g.name.slice(0, 2)} />
              <div className="pcard__title">
                <strong>{g.name}</strong>
                <span className="muted">For {uses.map((c) => c.name).join(", ")}</span>
              </div>
              {ready ? <span className="pill pill--ok">Set up</span> : <span className="pill pill--idle">Not set up</span>}
              <button type="button" className="btn btn--ghost btn--sm" aria-expanded={open === p} onClick={() => setOpen(open === p ? null : p)}>
                {open === p ? "Done" : "Set up"}
              </button>
            </div>
            {open === p && (
              <AppForm
                provider={p}
                guide={g}
                clientId={clientId}
                hasSecret={hasSecret}
                redirect={uses[0].redirect ?? ""}
                scopes={[...new Set(uses.flatMap((c) => c.scopes))]}
                onClientId={(id) => onChange({ ...value, [p]: { clientId: id } })}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

function AppForm(props: { provider: string; guide: Guide; clientId: string; hasSecret: boolean; redirect: string; scopes: string[]; onClientId: (id: string) => void }) {
  const { provider, guide: g, hasSecret } = props;
  const [clientId, setClientId] = useState(props.clientId);
  const [secret, setSecret] = useState("");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setClientId(props.clientId), [props.clientId]);
  const saveSecret = async (v: string) => {
    setError(null);
    try {
      await api.setAppSecret(provider, v);
      setSecret("");
    } catch (e) {
      setError(errorText(e));
    }
  };
  return (
    <div className="pform">
      <ol className="guide">
        {g.steps(props.redirect, props.scopes).map((s) => (
          <li key={s}>{s}</li>
        ))}
      </ol>
      <button type="button" className="btn btn--ghost btn--sm guide__open" onClick={() => api.openLink(g.url)}>
        Open {g.name}'s developer page
      </button>
      <div className="pform__grid">
        <label className="field">
          <span>Client ID</span>
          <input value={clientId} onChange={(e) => setClientId(e.target.value)} onBlur={() => clientId !== props.clientId && props.onClientId(clientId.trim())} />
        </label>
        {g.secret !== "none" && (
          <label className="field">
            <span>
              Client secret{g.secret === "optional" ? " (optional)" : ""}
              {hasSecret && " · saved"}
            </span>
            <input
              type="password"
              value={secret}
              placeholder={hasSecret ? "Saved in the keychain" : ""}
              onChange={(e) => setSecret(e.target.value)}
              onBlur={() => secret.trim() && saveSecret(secret)}
            />
          </label>
        )}
      </div>
      {hasSecret && (
        <button type="button" className="link-btn" onClick={() => saveSecret("")}>
          Remove the saved secret
        </button>
      )}
      {error && <p className="pcard__test is-err">{error}</p>}
    </div>
  );
}

// ---------- MCP servers ----------

/** A new id not used by any server: "linear", "linear-2", … */
function uniqueId(base: string, servers: McpServer[]) {
  const slug = base.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "server";
  let id = slug;
  for (let n = 2; servers.some((s) => s.id === id); n++) id = `${slug}-${n}`;
  return id;
}

function uniqueName(base: string, servers: McpServer[]) {
  let name = base;
  for (let n = 2; servers.some((s) => s.name === name); n++) name = `${base} ${n}`;
  return name;
}

function blankServer(servers: McpServer[]): McpServer {
  const name = uniqueName("My server", servers);
  return {
    id: uniqueId(name, servers),
    name,
    // Off until it's filled in; settings only check servers that are on.
    enabled: false,
    transport: "stdio",
    command: "",
    args: [],
    cwd: "",
    env: [],
    url: "",
    headers: [],
    oauth: false,
    oauthClientId: "",
    oauthScopes: "",
    permission: "readOnly",
    rules: {},
    preset: "",
  };
}

export function McpServers({ value, onChange }: { value: McpServer[]; onChange: (v: McpServer[]) => void }) {
  const [status, setStatus] = useState<ServerStatus[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  const [adding, setAdding] = useState<"menu" | "json" | null>(null);
  const [exported, setExported] = useState<string | null>(null);

  useEffect(() => {
    const load = () => void api.mcpStatus().then(setStatus, () => {});
    load();
    const off = listen(EVENTS.mcpChanged, load);
    return () => void off.then((f) => f());
  }, [value]);

  const add = (s: McpServer) => {
    onChange([...value, s]);
    setAdding(null);
    setOpen(s.id);
  };
  const addPreset = (p: Preset) => add({ ...p.server, id: uniqueId(p.server.id, value), name: uniqueName(p.server.name, value) });
  const update = (s: McpServer) => onChange(value.map((x) => (x.id === s.id ? s : x)));

  return (
    <div className="providers">
      {value.length === 0 && !adding && (
        <div className="providers__empty">
          <p>
            <strong>No MCP servers yet.</strong> They give agents more tools: Jira, Linear, AWS, a browser, and many more. Add one from the
            list, fill in your own, or paste the JSON other apps use.
          </p>
        </div>
      )}
      {value.map((s) => (
        <ServerCard
          key={s.id}
          server={s}
          status={status.find((x) => x.id === s.id)}
          expanded={open === s.id}
          onToggle={() => setOpen(open === s.id ? null : s.id)}
          onChange={update}
          onRemove={() => onChange(value.filter((x) => x.id !== s.id))}
        />
      ))}

      {adding === "menu" && <AddServer onPreset={addPreset} onCustom={() => add(blankServer(value))} onJson={() => setAdding("json")} onCancel={() => setAdding(null)} />}
      {adding === "json" && <ImportJson onDone={() => setAdding(null)} />}
      {!adding && (
        <div className="pform__foot">
          <button type="button" className="btn btn--ghost add-btn" onClick={() => setAdding("menu")}>
            + Add server
          </button>
          {value.length > 0 && (
            <button type="button" className="btn btn--ghost" onClick={async () => setExported(exported ? null : await api.mcpExport())}>
              {exported ? "Hide JSON" : "Show as JSON"}
            </button>
          )}
        </div>
      )}
      {exported && (
        <div className="textarea">
          <textarea readOnly value={exported} rows={Math.min(16, exported.split("\n").length)} className="mono" />
          <p className="muted">Secret values are left out as &lt;secret&gt;.</p>
        </div>
      )}
    </div>
  );
}

function AddServer(props: { onPreset: (p: Preset) => void; onCustom: () => void; onJson: () => void; onCancel: () => void }) {
  const [presets, setPresets] = useState<Preset[]>([]);
  useEffect(() => void api.mcpCatalog().then(setPresets, () => {}), []);
  return (
    <div className="add-provider">
      <div className="add-provider__head">
        <span>Add an MCP server</span>
        <button type="button" className="link-btn" onClick={props.onCancel}>
          Cancel
        </button>
      </div>
      <div className="preset-grid">
        {presets.map((p) => (
          <button key={p.id} type="button" className="preset" title={p.setup} onClick={() => props.onPreset(p)}>
            <Mark text={p.name.slice(0, 2)} />
            <span className="preset__name">{p.name}</span>
            <span className="preset__sub">{p.about}</span>
          </button>
        ))}
        <button type="button" className="preset" onClick={props.onCustom}>
          <Mark text="+" />
          <span className="preset__name">Your own</span>
          <span className="preset__sub">A program on this computer, or a server's address</span>
        </button>
        <button type="button" className="preset" onClick={props.onJson}>
          <Mark text="{}" />
          <span className="preset__name">Paste JSON</span>
          <span className="preset__sub">The mcpServers config other apps use</span>
        </button>
      </div>
    </div>
  );
}

function ImportJson({ onDone }: { onDone: () => void }) {
  const [text, setText] = useState("");
  const [report, setReport] = useState<{ ok: boolean; text: string } | null>(null);
  const run = async () => {
    try {
      const r = await api.mcpImport(text);
      const added = r.added.length ? `Added ${r.added.join(", ")}.` : "Nothing was added.";
      setReport({ ok: r.added.length > 0, text: [added, ...r.skipped].join(" ") });
      if (r.added.length && !r.skipped.length) onDone();
    } catch (e) {
      setReport({ ok: false, text: errorText(e) });
    }
  };
  return (
    <div className="add-provider">
      <div className="add-provider__head">
        <span>Paste MCP JSON</span>
        <button type="button" className="link-btn" onClick={onDone}>
          {report?.ok ? "Done" : "Cancel"}
        </button>
      </div>
      <div className="textarea">
        <textarea
          className="mono"
          rows={8}
          value={text}
          placeholder={'{\n  "mcpServers": {\n    "linear": { "url": "https://mcp.linear.app/mcp" }\n  }\n}'}
          onChange={(e) => setText(e.target.value)}
        />
      </div>
      <p className="muted">Keys, tokens and passwords in it go to the system keychain, not the settings file.</p>
      {report && <p className={`pcard__test ${report.ok ? "is-ok" : "is-err"}`}>{report.text}</p>}
      <button type="button" className="btn btn--primary add-btn" onClick={run} disabled={!text.trim()}>
        Import
      </button>
    </div>
  );
}

function ServerCard(props: { server: McpServer; status?: ServerStatus; expanded: boolean; onToggle: () => void; onChange: (s: McpServer) => void; onRemove: () => void }) {
  const { server: s, status, expanded, onToggle, onChange, onRemove } = props;
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const tools = status?.tools ?? [];

  const run = async (label: string, f: () => Promise<{ length: number }>) => {
    setBusy(label);
    setMessage(null);
    try {
      const t = await f();
      setMessage({ ok: true, text: `Connected. ${t.length} ${t.length === 1 ? "tool" : "tools"}.` });
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    }
    setBusy(null);
  };

  const pill =
    status?.state === "connected" ? (
      <span className="pill pill--ok">{tools.length} tools</span>
    ) : status?.state === "signIn" ? (
      <span className="pill pill--warn">Sign in</span>
    ) : status?.state === "error" ? (
      <span className="pill pill--err">Not working</span>
    ) : tools.length ? (
      <span className="pill pill--idle">{tools.length} tools</span>
    ) : (
      <span className="pill pill--idle">Not tried yet</span>
    );

  return (
    <div className={`pcard${expanded ? " is-open" : ""}`}>
      <div className="pcard__head">
        <Mark text={s.transport === "stdio" ? ">_" : "⇄"} />
        <div className="pcard__title">
          <strong>{s.name}</strong>
          <span className="mono muted">{s.transport === "stdio" ? [s.command, ...s.args].join(" ") || "No command yet" : s.url || "No address yet"}</span>
        </div>
        {pill}
        <Toggle id={`mcp-on-${s.id}`} checked={s.enabled} onChange={(enabled) => onChange({ ...s, enabled })} />
        {s.oauth && !status?.signedIn ? (
          <button type="button" className="btn btn--primary btn--sm" onClick={() => run("signin", () => api.mcpSignIn(s.id))} disabled={!!busy || !s.enabled}>
            {busy === "signin" ? "Waiting for the browser…" : "Sign in"}
          </button>
        ) : (
          <button type="button" className="btn btn--ghost btn--sm" onClick={() => run("connect", () => api.mcpConnect(s.id))} disabled={!!busy || !s.enabled}>
            {busy === "connect" ? "Connecting…" : tools.length ? "Refresh" : "Connect"}
          </button>
        )}
        <button type="button" className="btn btn--ghost btn--sm" aria-expanded={expanded} onClick={onToggle}>
          {expanded ? "Done" : "Edit"}
        </button>
      </div>
      {!s.enabled && !(s.transport === "stdio" ? s.command : s.url) && (
        <p className="pcard__note">Fill in {s.transport === "stdio" ? "the command" : "the address"} under Edit, then turn it on.</p>
      )}
      {status?.message && !message && <p className="pcard__test is-err">{status.message}</p>}
      {message && <p className={`pcard__test ${message.ok ? "is-ok" : "is-err"}`}>{message.text}</p>}
      {expanded && <ServerForm server={s} status={status} onChange={onChange} onRemove={onRemove} />}
    </div>
  );
}

function ServerForm({ server: s, status, onChange, onRemove }: { server: McpServer; status?: ServerStatus; onChange: (s: McpServer) => void; onRemove: () => void }) {
  const [name, setName] = useState(s.name);
  const [command, setCommand] = useState(s.command);
  const [args, setArgs] = useState(s.args.join("\n"));
  const [cwd, setCwd] = useState(s.cwd);
  const [url, setUrl] = useState(s.url);
  const [clientId, setClientId] = useState(s.oauthClientId);
  const [scopes, setScopes] = useState(s.oauthScopes);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const tools = status?.tools ?? [];
  const saved = status?.savedSecrets ?? [];
  const blur = (changed: boolean, patch: Partial<McpServer>) => changed && onChange({ ...s, ...patch });

  return (
    <div className="pform">
      <div className="pform__grid">
        <label className="field">
          <span>Name</span>
          <input value={name} onChange={(e) => setName(e.target.value)} onBlur={() => blur(name !== s.name, { name: name.trim() })} />
        </label>
        <div className="field">
          <span>Runs as</span>
          <Segmented
            id={`mcp-transport-${s.id}`}
            value={s.transport}
            options={[
              { value: "stdio", label: "Program on this computer" },
              { value: "http", label: "Remote server" },
            ]}
            onChange={(v) => onChange({ ...s, transport: v as McpServer["transport"] })}
          />
        </div>
      </div>

      {s.transport === "stdio" ? (
        <>
          <div className="pform__grid">
            <label className="field">
              <span>Command</span>
              <input className="mono" value={command} placeholder="npx, uvx, docker, or a full path" onChange={(e) => setCommand(e.target.value)} onBlur={() => blur(command !== s.command, { command: command.trim() })} />
            </label>
            <label className="field">
              <span>Working folder (optional)</span>
              <input className="mono" value={cwd} onChange={(e) => setCwd(e.target.value)} onBlur={() => blur(cwd !== s.cwd, { cwd: cwd.trim() })} />
            </label>
          </div>
          <label className="field">
            <span>Arguments, one per line</span>
            <div className="textarea">
              <textarea
                className="mono"
                rows={Math.max(2, args.split("\n").length)}
                value={args}
                onChange={(e) => setArgs(e.target.value)}
                onBlur={() => {
                  const list = args.split("\n").map((a) => a.trim()).filter(Boolean);
                  if (list.join("\n") !== s.args.join("\n")) onChange({ ...s, args: list });
                }}
              />
            </div>
          </label>
          <KeyValues title="Environment variables" kind="env" server={s} list={s.env} saved={saved} onChange={(env) => onChange({ ...s, env })} />
        </>
      ) : (
        <>
          <label className="field">
            <span>Address</span>
            <input className="mono" value={url} placeholder="https://example.com/mcp" onChange={(e) => setUrl(e.target.value)} onBlur={() => blur(url !== s.url, { url: url.trim() })} />
          </label>
          <div className="crow">
            <div>
              <div className="rules__label">Sign in with OAuth</div>
              <div className="rules__help">For servers like Jira or Linear that open a sign-in page. Servers that take a key use a header instead.</div>
            </div>
            <Toggle id={`mcp-oauth-${s.id}`} checked={s.oauth} onChange={(oauth) => onChange({ ...s, oauth })} />
          </div>
          {s.oauth && (
            <div className="pform__grid">
              <label className="field">
                <span>Client ID (only if the server needs one)</span>
                <input value={clientId} onChange={(e) => setClientId(e.target.value)} onBlur={() => blur(clientId !== s.oauthClientId, { oauthClientId: clientId.trim() })} />
              </label>
              <label className="field">
                <span>Scopes (optional)</span>
                <input value={scopes} onChange={(e) => setScopes(e.target.value)} onBlur={() => blur(scopes !== s.oauthScopes, { oauthScopes: scopes.trim() })} />
              </label>
            </div>
          )}
          {s.oauth && status?.signedIn && (
            <button type="button" className="link-btn" onClick={() => api.mcpSignOut(s.id)}>
              Sign out
            </button>
          )}
          <KeyValues title="Headers" kind="header" server={s} list={s.headers} saved={saved} onChange={(headers) => onChange({ ...s, headers })} />
        </>
      )}

      <div className="crow">
        <div>
          <div className="rules__label">Agents may use</div>
          <div className="rules__help">Read only offers just the tools the server marks as read-only.</div>
        </div>
        <Segmented
          id={`mcp-perm-${s.id}`}
          value={s.permission}
          options={[
            { value: "readOnly", label: "Read-only tools" },
            { value: "readWrite", label: "All tools" },
          ]}
          onChange={(v) => onChange({ ...s, permission: v as Permission })}
        />
      </div>
      {tools.length > 0 ? (
        <div className="rules">
          {tools.map((t) => {
            const off = !t.readOnly && s.permission === "readOnly";
            return (
              <div key={t.name} className={`rules__row${off ? " is-off" : ""}`}>
                <div>
                  <div className="rules__label mono">{t.name}</div>
                  <div className="rules__help">{off ? "Not offered while read-only" : t.description.split("\n")[0] || (t.readOnly ? "Only reads" : "May change something")}</div>
                </div>
                <Segmented
                  id={`mcp-rule-${s.id}-${t.name}`}
                  value={s.rules[t.name] ?? (t.readOnly ? "allow" : "ask")}
                  options={RULES}
                  onChange={(v) => onChange({ ...s, rules: { ...s.rules, [t.name]: v as Rule } })}
                />
              </div>
            );
          })}
        </div>
      ) : (
        <p className="pcard__note">Connect to see its tools and choose which ones ask first. Tools that can change things ask by default.</p>
      )}

      <div className="pform__foot">
        {confirmRemove ? (
          <>
            <span className="muted">Remove {s.name}?</span>
            <button type="button" className="btn btn--danger" onClick={onRemove}>
              Remove
            </button>
            <button type="button" className="btn btn--ghost" onClick={() => setConfirmRemove(false)}>
              Keep
            </button>
          </>
        ) : (
          <button type="button" className="btn btn--ghost" onClick={() => setConfirmRemove(true)}>
            Remove server
          </button>
        )}
      </div>
    </div>
  );
}

/** Environment variables or headers. Secret values go to the keychain. */
function KeyValues(props: { title: string; kind: "env" | "header"; server: McpServer; list: KeyValue[]; saved: string[]; onChange: (l: KeyValue[]) => void }) {
  const { title, kind, server, list, saved, onChange } = props;
  const [error, setError] = useState<string | null>(null);
  const set = (i: number, patch: Partial<KeyValue>) => onChange(list.map((kv, j) => (j === i ? { ...kv, ...patch } : kv)));
  const saveSecret = async (key: string, value: string) => {
    setError(null);
    try {
      await api.mcpSetSecret(server.id, kind, key, value);
    } catch (e) {
      setError(errorText(e));
    }
  };
  return (
    <div className="kv">
      <div className="rules__label">{title}</div>
      {list.map((kv, i) => (
        <KeyValueRow
          key={`${i}:${kv.key}`}
          kv={kv}
          kind={kind}
          saved={saved.includes(kv.key)}
          onChange={(patch) => set(i, patch)}
          onSecret={(v) => saveSecret(kv.key, v)}
          onRemove={() => {
            if (kv.secret) saveSecret(kv.key, "");
            onChange(list.filter((_, j) => j !== i));
          }}
        />
      ))}
      {error && <p className="pcard__test is-err">{error}</p>}
      <button type="button" className="link-btn" onClick={() => onChange([...list, { key: "", value: "", secret: kind === "header" }])}>
        + Add {kind === "env" ? "a variable" : "a header"}
      </button>
    </div>
  );
}

function KeyValueRow(props: { kv: KeyValue; kind: "env" | "header"; saved: boolean; onChange: (p: Partial<KeyValue>) => void; onSecret: (v: string) => void; onRemove: () => void }) {
  const { kv, kind, saved, onChange, onSecret, onRemove } = props;
  const [key, setKey] = useState(kv.key);
  const [value, setValue] = useState(kv.secret ? "" : kv.value);
  useEffect(() => setKey(kv.key), [kv.key]);
  return (
    <div className="kv__row">
      <input className="mono" value={key} placeholder={kind === "env" ? "NAME" : "Header-Name"} onChange={(e) => setKey(e.target.value)} onBlur={() => key !== kv.key && onChange({ key: key.trim() })} />
      <input
        className="mono"
        type={kv.secret ? "password" : "text"}
        value={value}
        placeholder={kv.secret ? (saved ? "Saved in the keychain" : "Secret value") : "Value"}
        onChange={(e) => setValue(e.target.value)}
        onBlur={() => {
          if (kv.secret) {
            if (value.trim()) {
              onSecret(value);
              setValue("");
            }
          } else if (value !== kv.value) onChange({ value });
        }}
      />
      <label className="kv__secret" title="Keep the value in the system keychain">
        <input
          type="checkbox"
          checked={kv.secret}
          onChange={(e) => {
            // Moving a value between the settings file and the keychain.
            if (e.target.checked) {
              if (value.trim()) onSecret(value);
              setValue("");
              onChange({ secret: true, value: "" });
            } else {
              onSecret("");
              onChange({ secret: false });
            }
          }}
        />
        Secret
      </label>
      <button type="button" className="link-btn" onClick={onRemove} aria-label={`Remove ${kv.key || "this row"}`}>
        Remove
      </button>
    </div>
  );
}
