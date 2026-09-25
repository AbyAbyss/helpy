import { useEffect, useState } from "react";
import type { Ai } from "../bindings/Ai";
import type { BuiltinProfile } from "../bindings/BuiltinProfile";
import type { Permissions } from "../bindings/Permissions";
import type { PlatformInfo } from "../bindings/PlatformInfo";
import type { Settings } from "../bindings/Settings";
import { api } from "../lib/ipc";
import { ProvidersEditor } from "./ai/Providers";

type Props = {
  settings: Settings;
  platform: PlatformInfo | null;
  commitAi: (ai: Ai) => Promise<string | null>;
  setProfile: (p: BuiltinProfile) => void;
  onDone: () => void;
};

const STEPS = ["Hello", "Model", "Profile", "Permissions", "Try it"] as const;

const PROFILES: { id: BuiltinProfile; title: string; text: string }[] = [
  { id: "beginner", title: "Beginner", text: "Explains things fully, reads answers and steps aloud, and tells you when agents finish." },
  { id: "expert", title: "Expert", text: "Short answers on screen, no voice unless you ask, labels without notes." },
  { id: "quiet", title: "Quiet", text: "No voice, no notifications, and as little motion as possible." },
];

/** The welcome tour, shown once in the settings window on a fresh install. */
export function Onboarding({ settings, platform, commitAi, setProfile, onDone }: Props) {
  const [step, setStep] = useState(0);
  const [perms, setPerms] = useState<Permissions | null>(null);
  useEffect(() => void api.permissions().then(setPerms, () => {}), [step]);

  const hasModel = settings.ai.routing.ask !== null;
  const mac = platform?.os === "macos";
  const last = step === STEPS.length - 1;
  const next = () => (last ? onDone() : setStep(step + 1));

  return (
    <div className="welcome">
      <div className="welcome__card" key={step}>
        <ol className="welcome__steps" aria-label="Steps">
          {STEPS.map((s, i) => (
            <li key={s} data-state={i < step ? "done" : i === step ? "now" : undefined}>
              {s}
            </li>
          ))}
        </ol>

        {step === 0 && (
          <section>
            <h1>Hi, I'm Helpy.</h1>
            <p className="welcome__lead">I sit next to your cursor and help with whatever's on your screen.</p>
            <ul className="welcome__list">
              <li>
                <strong>Ask anything</strong> out loud or by typing. I can look at your screen and point at the answer.
              </li>
              <li>
                <strong>Circle something</strong> to have it explained, copied or translated.
              </li>
              <li>
                <strong>Hand off chores</strong> to background agents: tidy a folder, research a purchase, build a small app.
              </li>
            </ul>
            <p className="welcome__small">Setup takes about a minute. You can change everything later in Settings.</p>
          </section>
        )}

        {step === 1 && (
          <section>
            <h1>Choose the AI I use</h1>
            <p className="welcome__lead">
              Add a model on this computer (Ollama, LM Studio) for full privacy, or a cloud provider with your own API key. Keys stay in your
              system's keychain.
            </p>
            <ProvidersEditor ai={settings.ai} commit={commitAi} />
            {hasModel && <p className="welcome__ok">Ready. I'll answer with {settings.ai.routing.ask?.model}.</p>}
          </section>
        )}

        {step === 2 && (
          <section>
            <h1>How should I behave?</h1>
            <p className="welcome__lead">Pick a starting point. Switch any time from the tray menu.</p>
            <div className="welcome__choices" role="radiogroup" aria-label="Behavior profile">
              {PROFILES.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  role="radio"
                  aria-checked={settings.profiles.active === p.id}
                  className="welcome__choice"
                  onClick={() => setProfile(p.id)}
                >
                  <strong>{p.title}</strong>
                  <span>{p.text}</span>
                </button>
              ))}
            </div>
          </section>
        )}

        {step === 3 && (
          <section>
            <h1>{mac ? "Two permissions" : "What works here"}</h1>
            {mac ? (
              <>
                <p className="welcome__lead">macOS asks you to allow these once. Nothing is shared until you ask me something.</p>
                <div className="welcome__perm">
                  <div>
                    <strong>Screen Recording</strong>
                    <span>So I can see what you ask about. macOS asks the first time I look.</span>
                  </div>
                  <button type="button" className="btn btn--sm" onClick={() => api.permissionsOpen("screen")}>
                    Open settings
                  </button>
                </div>
                <div className="welcome__perm">
                  <div>
                    <strong>Accessibility</strong>
                    <span>So highlights land on the exact button, and for "Do it for me".</span>
                  </div>
                  {perms?.accessibility ? (
                    <span className="pill pill--ok">Allowed</span>
                  ) : (
                    <button type="button" className="btn btn--sm" onClick={() => api.permissionsRequest().then(setPerms)}>
                      Allow…
                    </button>
                  )}
                </div>
              </>
            ) : platform && platform.limitations.length > 0 ? (
              <>
                <p className="welcome__lead">No permissions to grant, but this desktop limits a few things:</p>
                <ul className="welcome__list">
                  {platform.limitations.map((l) => (
                    <li key={l}>{l}</li>
                  ))}
                </ul>
              </>
            ) : (
              <p className="welcome__lead">Nothing to allow on this system. Everything works as it is.</p>
            )}
          </section>
        )}

        {step === 4 && (
          <section>
            <h1>Try it</h1>
            <p className="welcome__lead">These work from any app. Change them under Hotkeys.</p>
            <dl className="welcome__keys">
              <dt>
                <kbd>{settings.hotkeys.voiceAsk}</kbd>
              </dt>
              <dd>Ask out loud</dd>
              <dt>
                <kbd>{settings.hotkeys.textAsk}</kbd>
              </dt>
              <dd>Type a question</dd>
              <dt>
                <kbd>{settings.hotkeys.circleToExplain}</kbd>
              </dt>
              <dd>Circle something on screen</dd>
              <dt>
                <kbd>{settings.hotkeys.openAgentPanel}</kbd>
              </dt>
              <dd>Agents</dd>
            </dl>
            <p className="welcome__small">I'll wait in the tray. Settings are there too.</p>
          </section>
        )}

        <footer className="welcome__foot">
          {step > 0 ? (
            <button type="button" className="btn btn--ghost" onClick={() => setStep(step - 1)}>
              Back
            </button>
          ) : (
            <button type="button" className="link-btn" onClick={onDone}>
              Skip the tour
            </button>
          )}
          <span className="welcome__spacer" />
          {step === 1 && !hasModel && (
            <button type="button" className="link-btn" onClick={next}>
              Later
            </button>
          )}
          <button type="button" className="btn btn--primary" onClick={next} disabled={step === 1 && !hasModel}>
            {step === 0 ? "Get started" : last ? "Start using Helpy" : "Continue"}
          </button>
        </footer>
      </div>
    </div>
  );
}
