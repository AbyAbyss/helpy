import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AskEvent } from "../bindings/AskEvent";
import type { VoicePhase } from "../bindings/VoicePhase";
import { api, EVENTS } from "../lib/ipc";
import { captionText, initial, onAsk, onPartial, onVoice, type PillState } from "./pillState";

const BARS = 9;
/** Middle bars move most, like a voice spectrum. */
const PROFILE = [0.35, 0.55, 0.8, 0.95, 1, 0.95, 0.8, 0.55, 0.35];
/** How long a finished answer or a message stays before the pill hides. */
const HIDE_AFTER = { done: 7000, message: 3500, error: 6000 };

/**
 * The compact voice view: a glowing waveform next to the cursor, with a
 * one-line caption. Clicking it opens the full conversation.
 */
export function Pill() {
  const [state, setState] = useState<PillState>(initial);
  const [speaking, setSpeaking] = useState(false);
  const [hover, setHover] = useState(false);
  const level = useRef(0);
  const voiceTurn = useRef(false);
  const bars = useRef<(HTMLSpanElement | null)[]>([]);
  const root = useRef<HTMLDivElement>(null);
  const mode = useRef(state.mode);
  const speakingRef = useRef(speaking);
  mode.current = state.mode;
  speakingRef.current = speaking;

  useEffect(() => {
    const offs = [
      listen<VoicePhase>(EVENTS.voiceState, (e) => {
        if (e.payload.phase === "listening") voiceTurn.current = false;
        setState((s) => onVoice(s, e.payload));
      }),
      listen<number>(EVENTS.voiceLevel, (e) => (level.current = e.payload)),
      listen<string>(EVENTS.voicePartial, (e) => setState((s) => onPartial(s, e.payload))),
      listen<boolean>(EVENTS.speaking, (e) => setSpeaking(e.payload)),
      listen<string>(EVENTS.speakError, (e) => setState({ mode: "message", caption: e.payload, tone: "error" })),
      listen<AskEvent>(EVENTS.ask, ({ payload }) => {
        if (payload.type === "question") voiceTurn.current = payload.voice;
        setState((s) => onAsk(s, payload, voiceTurn.current));
      }),
    ];
    return () => offs.forEach((o) => void o.then((f) => f()));
  }, []);

  // Hide once there's nothing more to show, unless the pointer is on it.
  useEffect(() => {
    if (hover || speaking) return;
    const delay =
      state.mode === "done" ? HIDE_AFTER.done : state.mode === "message" ? (state.tone === "error" ? HIDE_AFTER.error : HIDE_AFTER.message) : null;
    if (delay === null) return;
    const t = setTimeout(() => api.voicePillHide(), delay);
    return () => clearTimeout(t);
  }, [state, hover, speaking]);

  // The window is only as big as what it shows, so it never blocks clicks
  // on the app underneath beyond its own content.
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

  // Bars are animated straight on the DOM, 60 times a second.
  useEffect(() => {
    let raf = 0;
    const shown = new Array(BARS).fill(0.1);
    const tick = (t: number) => {
      for (let i = 0; i < BARS; i++) {
        let target: number;
        const wobble = 0.65 + 0.35 * Math.sin(t / 90 + i * 1.7);
        switch (mode.current) {
          case "listening":
            target = 0.12 + level.current * PROFILE[i] * wobble;
            break;
          case "working":
            // A wave travelling across the bars.
            target = 0.18 + 0.28 * (0.5 + 0.5 * Math.sin(t / 160 - i * 0.7));
            break;
          case "answering":
          case "done":
            target = speakingRef.current ? 0.2 + 0.6 * PROFILE[i] * (0.5 + 0.5 * Math.sin(t / 70 + i * 2.3)) : 0.14 + 0.06 * Math.sin(t / 400 + i);
            break;
          default:
            target = 0.12;
        }
        shown[i] += (target - shown[i]) * 0.35;
        const el = bars.current[i];
        if (el) el.style.transform = `scaleY(${Math.max(0.1, Math.min(1, shown[i])).toFixed(3)})`;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  const caption = state.mode === "answering" || state.mode === "done" ? captionText(state.caption) : state.caption;
  const tone = state.tone === "error" ? "error" : state.mode === "done" ? "done" : "working";
  const label =
    state.mode === "listening" ? "Listening" : state.mode === "working" ? "Thinking" : state.mode === "message" ? caption : "Answer";

  return (
    <div
      ref={root}
      className={`pill pill--${tone} pill--${state.mode}`}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      onClick={() => api.voiceExpand()}
      role="button"
      aria-label={`${label}. Click to open the conversation`}
    >
      <div className="wave" aria-hidden="true">
        {Array.from({ length: BARS }, (_, i) => (
          <span key={i} ref={(el) => void (bars.current[i] = el)} />
        ))}
      </div>
      {caption && (
        <div className="caption">
          <span className="caption__text">{caption}</span>
          {hover && state.mode !== "listening" && <span className="caption__open">Open</span>}
        </div>
      )}
    </div>
  );
}
