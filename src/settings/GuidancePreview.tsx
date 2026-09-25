import { useLayoutEffect, useRef, useState } from "react";
import type { Guidance } from "../bindings/Guidance";
import type { Mark } from "../bindings/Mark";
import { Annotations } from "../guide/Annotations";
import { MockApp } from "./BuddyPreview";

type Sample = "highlight" | "point" | "arrow";

const SAMPLES: { id: Sample; label: string }[] = [
  { id: "highlight", label: "Highlight" },
  { id: "point", label: "Pointer" },
  { id: "arrow", label: "Arrow" },
];

/** Shows the current look on a sketched mail app, pointing at "Junk Email". */
export function GuidanceStage({ look }: { look: Guidance }) {
  const stage = useRef<HTMLDivElement>(null);
  const [sample, setSample] = useState<Sample>("highlight");
  const [replay, setReplay] = useState(0);
  const [box, setBox] = useState<{ w: number; h: number; target: DOMRect | null }>({ w: 0, h: 0, target: null });

  // Marks are placed on the real layout of the sketch, like on a screen.
  useLayoutEffect(() => {
    const el = stage.current;
    if (!el) return;
    const measure = () => {
      const s = el.getBoundingClientRect();
      const t = el.querySelector(".mock__nav b:nth-child(4)")?.getBoundingClientRect();
      setBox({ w: s.width, h: s.height, target: t ? new DOMRect(t.x - s.x, t.y - s.y, t.width, t.height) : null });
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const t = box.target;
  const marks: Mark[] = !t
    ? []
    : sample === "highlight"
      ? [{ type: "highlight", x: t.x, y: t.y, width: t.width, height: t.height, label: "Junk Email", raw: "96,212 104x24" }]
      : sample === "point"
        ? [{ type: "point", x: t.x + t.width / 2, y: t.y + 4, label: "Your spam is here", raw: "148,216" }]
        : [{ type: "arrow", fromX: t.right + 150, fromY: t.bottom + 34, toX: t.right + 6, toY: t.y + t.height / 2, label: "Open this folder", raw: "402,270 → 206,224" }];

  return (
    <div className="stage-wrap">
      <div ref={stage} className="stage stage--guide">
        <MockApp />
        {marks.length > 0 && <Annotations key={`${sample}-${replay}`} marks={marks} width={box.w} height={box.h} look={look} />}
      </div>
      <div className="stage__states" role="radiogroup" aria-label="Preview mark">
        {SAMPLES.map((s) => (
          <button key={s.id} type="button" role="radio" aria-checked={sample === s.id} className="chip" onClick={() => setSample(s.id)}>
            {s.label}
          </button>
        ))}
        <button type="button" className="link-btn stage__replay" onClick={() => setReplay((r) => r + 1)}>
          Replay
        </button>
      </div>
    </div>
  );
}
