import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { CursorFrame } from "../bindings/CursorFrame";
import type { Mark } from "../bindings/Mark";
import type { MarksView } from "../bindings/MarksView";
import type { AskActivity } from "../bindings/AskActivity";
import type { BuddyHandoff } from "../bindings/BuddyHandoff";
import type { BuddyStyle } from "../bindings/BuddyStyle";
import { Buddy } from "../buddy/Buddy";
import { followStep, type Point } from "../buddy/follow";
import { handoffFrames, planTour, tourAt, traceOf, type Tour } from "../buddy/tour";
import { CircleLayer } from "../circle/CircleLayer";
import { Annotations } from "../guide/Annotations";
import { api, EVENTS } from "../lib/ipc";
import { useSettings } from "../lib/useSettings";

/** How long the buddy stays on the last mark before going back to the cursor, ms. */
const TOUR_HOLD = 900;
/** How smoothly it glides back, whatever the follow smoothness is. */
const RETURN_SMOOTHNESS = 0.9;
const RETURN_MS = 700;
/** The hand-off flight, ms. The dock holds the new chip back to match. */
const HANDOFF_MS = 820;

/** Where the buddy's point is, from its top-left corner: Pip's tip, or the middle. */
function tipOf(style: BuddyStyle, size: number): number {
  return style === "pip" ? (size * 6) / 64 : size / 2;
}

/** The click-through layer on one monitor: guidance marks and the buddy. */
export function Overlay() {
  const [settings] = useSettings();
  const [visible, setVisible] = useState(false);
  const [inside, setInside] = useState(false);
  const [customSrc, setCustomSrc] = useState<string | null>(null);
  const [marks, setMarks] = useState<{ id: number; marks: Mark[] }>({ id: 0, marks: [] });
  const [circling, setCircling] = useState(false);
  // Agents at work, for the buddy's badge.
  const [working, setWorking] = useState(0);
  // A question being answered, and how many wait behind it.
  const [activity, setActivity] = useState<AskActivity>({ running: false, queued: 0 });
  const el = useRef<HTMLDivElement>(null);
  const target = useRef<Point | null>(null);
  const pos = useRef<Point | null>(null);
  const smoothness = useRef(0);
  smoothness.current = settings?.buddy.smoothness ?? 0;
  // The buddy's walk over the current step's marks, while it plays.
  const tour = useRef<{ plan: Tour; start: number } | null>(null);
  const [touring, setTouring] = useState(false);
  const [timing, setTiming] = useState<{ draw: number; dur: number }[] | undefined>();
  const returning = useRef(0);
  // Copies flying off to the dock, and the buddy's flash as one splits off.
  const [handoffs, setHandoffs] = useState<{ id: number; to: Point; from: Point }[]>([]);
  const [splitting, setSplitting] = useState(0);
  // Read from event handlers, which are set up once.
  const live = useRef({ visible, inside, settings });
  live.current = { visible, inside, settings };

  /** Sends the buddy over a step's marks as they draw, when it's showing and motion is on. */
  const startTour = (marks: Mark[]) => {
    const { visible, settings } = live.current;
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const drawn = marks.length > 0 && !!settings && visible && !settings.guidance.reduceMotion && !reduced;
    if (!drawn) {
      if (tour.current) returning.current = performance.now();
      tour.current = null;
      setTouring(false);
      setTiming(undefined);
      return;
    }
    const b = settings.buddy;
    const curved = settings.guidance.arrowStyle === "curved";
    // From the buddy's point where it is now, or from the first mark when
    // the cursor is on another monitor.
    const from = pos.current
      ? { x: pos.current.x + b.offsetX + tipOf(b.style, b.size), y: pos.current.y + b.offsetY + tipOf(b.style, b.size) }
      : traceOf(marks[0], curved)[0];
    const plan = planTour(marks, from, 1 / settings.guidance.animationSpeed, curved);
    tour.current = { plan, start: performance.now() };
    setTouring(true);
    setTiming(plan.legs.map((l) => ({ draw: l.draw, dur: l.dur })));
  };

  useEffect(() => {
    // Cursor frames are sent to this window only, so listen on the window.
    const offCursor = getCurrentWebviewWindow().listen<CursorFrame>(EVENTS.cursor, ({ payload }) => {
      setInside(payload.inside);
      if (!payload.inside) {
        // Re-enter from the new spot instead of gliding across the screen.
        target.current = pos.current = null;
        return;
      }
      target.current = { x: payload.x, y: payload.y };
      pos.current ??= target.current;
    });
    const offVisible = listen<boolean>(EVENTS.buddyVisible, (e) => setVisible(e.payload));
    // Every overlay hears each step; only the one on the step's monitor draws it.
    const self = getCurrentWebviewWindow().label;
    const offMarks = listen<MarksView>(EVENTS.guideMarks, ({ payload }) => {
      const mine = payload.overlay === self ? payload.marks : [];
      setMarks((m) => ({ id: m.id + 1, marks: mine }));
      startTour(mine);
    });
    const offClear = listen(EVENTS.clearAnnotations, () => {
      setMarks((m) => ({ id: m.id + 1, marks: [] }));
      startTour([]);
    });
    const offHandoff = getCurrentWebviewWindow().listen<BuddyHandoff>(EVENTS.buddyHandoff, ({ payload }) => {
      const { visible, inside, settings } = live.current;
      // Only the overlay showing the buddy flies a copy off it.
      if (!settings || !visible || !inside || tour.current || !pos.current) return;
      if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
      const b = settings.buddy;
      const from = { x: pos.current.x + b.offsetX + b.size / 2, y: pos.current.y + b.offsetY + b.size / 2 };
      const id = performance.now();
      setHandoffs((h) => [...h, { id, from, to: { x: payload.x, y: payload.y } }]);
      setSplitting(id);
    });
    const offCount = listen<number>(EVENTS.agentCount, (e) => setWorking(e.payload));
    const offActivity = listen<AskActivity>(EVENTS.askActivity, (e) => setActivity(e.payload));
    api.askActivity().then(setActivity, () => {});
    api.agents().then((l) => setWorking(l.agents.filter((a) => !["done", "failed", "stopped", "cancelled", "ready"].includes(a.status)).length));
    api.overlayReady().then(setVisible);
    return () => {
      offCursor.then((f) => f());
      offVisible.then((f) => f());
      offMarks.then((f) => f());
      offClear.then((f) => f());
      offHandoff.then((f) => f());
      offCount.then((f) => f());
      offActivity.then((f) => f());
    };
  }, []);

  const style = settings?.buddy.style;
  useEffect(() => {
    if (style !== "custom") return;
    const load = () => api.customBuddy().then(setCustomSrc);
    load();
    const off = listen(EVENTS.buddyImageChanged, load);
    return () => void off.then((f) => f());
  }, [style]);

  // The marks' draw-in starts when they mount, so the tour's clock does too.
  useLayoutEffect(() => {
    if (tour.current) tour.current.start = performance.now();
  }, [marks.id]);

  // Position is written straight to the DOM each frame; no React renders.
  useEffect(() => {
    let raf = 0;
    let last = performance.now();
    const tick = (now: number) => {
      const dt = Math.min(now - last, 100);
      last = now;
      const t = tour.current;
      if (t && el.current) {
        // Touring: the element sits on the buddy's point, offset by the tip (see render).
        const elapsed = now - t.start;
        const p = tourAt(t.plan, elapsed);
        el.current.style.transform = `translate3d(${p.x}px, ${p.y}px, 0)`;
        if (elapsed >= t.plan.end + TOUR_HOLD) {
          // Glide back from here: the cursor position is the point minus the offset.
          const b = live.current.settings?.buddy;
          pos.current = b ? { x: p.x - b.offsetX - tipOf(b.style, b.size), y: p.y - b.offsetY - tipOf(b.style, b.size) } : p;
          tour.current = null;
          returning.current = now;
          setTouring(false);
        }
      } else if (target.current && pos.current && el.current) {
        const back = now - returning.current < RETURN_MS;
        pos.current = followStep(pos.current, target.current, back ? Math.max(RETURN_SMOOTHNESS, smoothness.current) : smoothness.current, dt);
        el.current.style.transform = `translate3d(${pos.current.x}px, ${pos.current.y}px, 0)`;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  if (!settings) return null;
  const b = settings.buddy;
  // The buddy steps aside while a selection is being drawn over its spot.
  const shown = visible && (inside || touring) && !circling;
  const tip = tipOf(b.style, b.size);

  return (
    <>
      {marks.marks.length > 0 && (
        // Keyed per step so its draw-in animation plays again on Repeat.
        <Annotations key={marks.id} marks={marks.marks} width={window.innerWidth} height={window.innerHeight} look={settings.guidance} timing={timing} />
      )}
      <CircleLayer color={settings.guidance.highlightColor} onOpen={setCircling} />
      <div ref={el} className="overlay-buddy" style={{ opacity: shown ? b.opacity : 0 }}>
        {/* Beside the cursor normally; on its point while touring marks. */}
        <div className="overlay-buddy__offset" style={{ transform: touring ? `translate(${-tip}px, ${-tip}px)` : `translate(${b.offsetX}px, ${b.offsetY}px)` }}>
          <div
            key={splitting}
            className={splitting ? "overlay-buddy__split" : undefined}
            onAnimationEnd={(e) => e.target === e.currentTarget && setSplitting(0)}
          >
            <Buddy
              style={b.style}
              size={b.size}
              state={activity.running ? "thinking" : "idle"}
              animate={b.showStateAnimations}
              badge={b.showAgentBadge && working > 0 ? working : null}
              queued={activity.queued}
              customSrc={customSrc}
            />
          </div>
        </div>
      </div>
      {handoffs.map((h) => (
        <Handoff
          key={h.id}
          from={h.from}
          to={h.to}
          style={b.style}
          size={b.size}
          customSrc={customSrc}
          onDone={() => setHandoffs((l) => l.filter((x) => x.id !== h.id))}
        />
      ))}
    </>
  );
}

/**
 * A copy of the buddy that turns the working colour, peels off and flies to
 * the dock, where it becomes the new agent's chip.
 */
function Handoff({ from, to, style, size, customSrc, onDone }: { from: Point; to: Point; style: BuddyStyle; size: number; customSrc: string | null; onDone: () => void }) {
  const el = useRef<HTMLDivElement>(null);
  const done = useRef(onDone);
  done.current = onDone;
  useEffect(() => {
    const node = el.current;
    if (!node) return;
    const frames = handoffFrames(from, to).map((f) => ({
      offset: f.offset,
      opacity: f.opacity,
      transform: `translate3d(${f.x - size / 2}px, ${f.y - size / 2}px, 0) rotate(${f.angle}deg) scale(${f.scale})`,
    }));
    const a = node.animate(frames, { duration: HANDOFF_MS, easing: "cubic-bezier(0.45, 0, 0.25, 1)", fill: "forwards" });
    a.onfinish = () => done.current();
    return () => a.cancel();
  }, [from, to, size]);
  return (
    <div ref={el} className="overlay-handoff">
      <Buddy style={style} size={size} animate={false} customSrc={customSrc} />
    </div>
  );
}
