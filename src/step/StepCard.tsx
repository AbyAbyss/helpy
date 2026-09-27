import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { CardView } from "../bindings/CardView";
import { api, EVENTS } from "../lib/ipc";
import { useSettings } from "../lib/useSettings";

/**
 * The walkthrough card: the only part of a walkthrough that takes clicks.
 * Everything drawn on the overlay stays click-through.
 */
export function StepCard() {
  const [card, setCard] = useState<CardView | null>(null);
  const [settings] = useSettings();
  const [glass, setGlass] = useState<string | null>(null);
  // Shrunk to its header so it covers less of the screen.
  const [mini, setMini] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.guideCardGlass().then(setGlass);
    api.guideCard().then((c) => c && setCard(c));
    const off = listen<CardView>(EVENTS.guideCard, (e) => setCard(e.payload));
    return () => void off.then((f) => f());
  }, []);

  // The window is only as big as the card.
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

  const voice = settings?.voiceOutput.voiceGuidance ?? false;
  // A new walkthrough starts with the full card.
  useEffect(() => {
    if (card?.number === 1) setMini(false);
  }, [card?.number]);

  // Drag the card by its header; Helpy then leaves it where it's put.
  const drag = (e: React.MouseEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    getCurrentWindow().startDragging().catch(() => {});
    api.guideAction("moved");
  };

  const toggleVoice = () => {
    api.set("voiceOutput.voiceGuidance", !voice);
    if (voice) api.stopSpeaking();
  };

  return (
    <div ref={root} className="step" data-glass={glass ?? undefined} data-still={settings?.guidance.reduceMotion || undefined}>
      {card && (
        <div className="card" key={card.number} data-mini={mini || undefined} role="dialog" aria-live="polite" aria-label={`Step ${card.number}`}>
          <header className="card__head" title="Drag to move" onMouseDown={drag}>
            <Mark />
            <span className="card__count">
              Step {card.number}
              {card.total != null && <> of {card.total}</>}
            </span>
            {card.total != null && card.total > 1 && <Progress number={card.number} total={card.total} />}
            <span className="card__tools">
              {mini && card.playing && !card.finished && (
                <button type="button" className="icon-btn" title={card.paused ? "Play" : "Pause"} onClick={() => api.guideAction("pause")}>
                  {card.paused ? <PlayIcon /> : <PauseIcon />}
                </button>
              )}
              {!mini && (
                <button
                  type="button"
                  className="icon-btn"
                  aria-pressed={voice}
                  title={voice ? "Stop reading steps aloud" : "Read steps aloud"}
                  onClick={toggleVoice}
                >
                  <SpeakerIcon on={voice} />
                </button>
              )}
              <button type="button" className="icon-btn" title={mini ? "Show the whole card" : "Make smaller"} onClick={() => setMini(!mini)}>
                {mini ? <ExpandIcon /> : <MinimizeIcon />}
              </button>
            </span>
          </header>

          {!mini && (
            <>

          <p className="card__text">{card.instruction}</p>

          {card.checking ? (
            <p className="card__hint card__hint--busy">
              <span className="spinner" aria-hidden="true" />
              Checking your screen…
            </p>
          ) : card.playing ? (
            <p className={`card__hint${card.waiting && !card.reviewing ? " card__hint--busy" : ""}`}>
              {card.waiting && !card.reviewing && <span className="spinner" aria-hidden="true" />}
              {card.reviewing
                ? "Going back through it. Next moves forward."
                : card.finished
                  ? "That's all of it. Go back through it, or press Done."
                  : card.paused
                    ? "Paused. Press Play to carry on."
                    : card.waiting
                      ? "Getting the next step…"
                      : "Plays on by itself. Pause to stay on this step."}
            </p>
          ) : card.confirming ? (
            <p className="card__hint card__hint--ask">Helpy will click the marked spot once. Is that the right place?</p>
          ) : (
            <p className="card__hint">
              {card.clickAdvances ? "Click the highlighted spot, or press Next." : "Press Next once you've done it."}
            </p>
          )}
          {card.problem && !card.confirming && <p className="card__hint card__hint--err">{card.problem}</p>}

          {card.confirming ? (
            <footer className="card__actions">
              <button type="button" className="btn btn--quiet" onClick={() => api.guideAction("back")}>
                Back
              </button>
              <span className="card__spacer" />
              <button type="button" className="btn btn--primary" autoFocus onClick={() => api.guideAction("confirm")}>
                <ClickIcon />
                Click it
              </button>
            </footer>
          ) : (
            <footer className="card__actions">
              <button type="button" className="btn btn--quiet" onClick={() => api.guideAction("stop")}>
                Stop <kbd>Esc</kbd>
              </button>
              <span className="card__spacer" />
              <button
                type="button"
                className="btn"
                disabled={card.checking}
                title="Repeat"
                aria-label="Repeat"
                onClick={() => api.guideAction("repeat")}
              >
                <RepeatIcon />
                {!card.canDoIt && "Repeat"}
              </button>
              {card.canBack && (
                <button type="button" className="btn" title="Previous step" aria-label="Previous step" onClick={() => api.guideAction("prev")}>
                  <BackIcon />
                </button>
              )}
              {card.playing && !card.finished && !card.reviewing && (
                <button type="button" className="btn" onClick={() => api.guideAction("pause")}>
                  {card.paused ? <PlayIcon /> : <PauseIcon />}
                  {card.paused ? "Play" : "Pause"}
                </button>
              )}
              {card.canDoIt && (
                <button type="button" className="btn" disabled={card.checking} onClick={() => api.guideAction("doIt")}>
                  <ClickIcon />
                  Do it
                </button>
              )}
              <button type="button" className="btn btn--primary" disabled={card.checking} onClick={() => api.guideAction("next")}>
                {card.finished && !card.reviewing ? "Done" : "Next"}
              </button>
            </footer>
          )}
            </>
          )}
        </div>
      )}
    </div>
  );
}

function Progress({ number, total }: { number: number; total: number }) {
  return (
    <span className="progress" aria-hidden="true">
      {Array.from({ length: Math.min(total, 12) }, (_, i) => (
        <span key={i} className="progress__seg" data-state={i + 1 < number ? "done" : i + 1 === number ? "now" : undefined} />
      ))}
    </span>
  );
}

function Mark() {
  return (
    <svg className="card__mark" viewBox="0 0 64 64" width="18" height="18" aria-hidden="true">
      <path d="M6 6 L26 13.5 A22 22 0 1 1 13.5 26 Z" fill="var(--accent)" />
      <ellipse cx="29" cy="33" rx="3.2" ry="4.4" fill="#141821" />
      <ellipse cx="41" cy="33" rx="3.2" ry="4.4" fill="#141821" />
    </svg>
  );
}

function SpeakerIcon({ on }: { on: boolean }) {
  return (
    <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true">
      <path d="M3.5 7.5h3l4-3.5v12l-4-3.5h-3z" />
      {on ? <path d="M13.5 7a4 4 0 0 1 0 6M15.5 5a7 7 0 0 1 0 10" /> : <path d="m13.5 8 4 4m0-4-4 4" />}
    </svg>
  );
}

function ClickIcon() {
  return (
    <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
      <path d="M8 3.5v2M3.5 8h2M4.8 4.8l1.4 1.4M9 9l7.5 3-3.2 1.3L12 16.5z" />
    </svg>
  );
}

function PauseIcon() {
  return (
    <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
      <path d="M7 4.5v11M13 4.5v11" />
    </svg>
  );
}

function PlayIcon() {
  return (
    <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
      <path d="M6.5 4.5v11l9-5.5z" />
    </svg>
  );
}

function BackIcon() {
  return (
    <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
      <path d="M12 4.5 6.5 10l5.5 5.5" />
    </svg>
  );
}

function MinimizeIcon() {
  return (
    <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true">
      <path d="M5 10h10" />
    </svg>
  );
}

function ExpandIcon() {
  return (
    <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true">
      <path d="M5.5 12.5 10 8l4.5 4.5" />
    </svg>
  );
}

function RepeatIcon() {
  return (
    <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
      <path d="M4 10a6 6 0 0 1 10.2-4.3L16 7.5M16 3.5v4h-4M16 10a6 6 0 0 1-10.2 4.3L4 12.5M4 16.5v-4h4" />
    </svg>
  );
}
