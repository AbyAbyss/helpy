// 16px line icons for the settings sidebar. Drawn on a 16 grid, 1.5 stroke.

export type IconName =
  | "sliders" | "layers" | "buddy" | "keyboard" | "chip" | "chat" | "target" | "lasso"
  | "mic" | "speaker" | "agents" | "plug" | "shield" | "meter" | "wrench"
  | "search" | "upload" | "download" | "reset" | "check" | "alert" | "monitor";

const P: Record<IconName, string> = {
  sliders: "M2.5 4.5h6M11.5 4.5h2M2.5 11.5h2M7.5 11.5h6M10 3v3M6 10v3",
  layers: "M8 2.5 13.5 5.5 8 8.5 2.5 5.5ZM2.5 8.5 8 11.5l5.5-3M2.5 11 8 14l5.5-3",
  buddy: "M3 3c2 .3 3.6.8 4.6 1.5A5 5 0 1 1 4 8.2c-.6-1.4-.9-3.2-1-5.2ZM7.3 9v.6M10.2 9v.6",
  keyboard: "M1.5 4.5h13v7h-13ZM4 7h.5M6.5 7H7M9 7h.5M11.5 7h.5M5 9.5h6",
  chip: "M4.5 4.5h7v7h-7ZM6.5 2v2.5M9.5 2v2.5M6.5 11.5V14M9.5 11.5V14M2 6.5h2.5M2 9.5h2.5M11.5 6.5H14M11.5 9.5H14",
  chat: "M2.5 3.5h11v7.5H7l-3 2.5V11H2.5Z",
  target: "M8 2.5v2M8 11.5v2M2.5 8h2M11.5 8h2M8 5a3 3 0 1 1 0 6 3 3 0 0 1 0-6Z",
  lasso: "M8 3c3.3 0 5.5 1.6 5.5 3.5S11.3 10 8 10 2.5 8.4 2.5 6.5 4.7 3 8 3ZM5 9.5c-.8 1-.6 2.4.6 3 1 .5 2 .2 2.4-.5",
  mic: "M6 2.5h4v6.5H6ZM3.5 7.5a4.5 4.5 0 0 0 9 0M8 12v2",
  speaker: "M2.5 6h2.5l3.5-3v10L5 10H2.5ZM11 5.5a3.5 3.5 0 0 1 0 5M12.8 3.8a6 6 0 0 1 0 8.4",
  agents: "M5.5 3.5a2 2 0 1 1 0 4 2 2 0 0 1 0-4ZM11 5a1.6 1.6 0 1 1 0 3.2A1.6 1.6 0 0 1 11 5ZM2 13c.4-2.2 1.8-3.5 3.5-3.5S8.6 10.8 9 13M9.5 10.3c.5-.5 1-.8 1.5-.8 1.4 0 2.6 1.1 3 3",
  plug: "M6 2v3M10 2v3M4.5 5h7v3a3.5 3.5 0 0 1-7 0ZM8 11.5V14",
  shield: "M8 2 13 4v4c0 3-2.2 5.2-5 6-2.8-.8-5-3-5-6V4Z",
  meter: "M2.5 12a5.5 5.5 0 1 1 11 0M8 12l2.5-3.5",
  wrench: "M10.5 2.5a3 3 0 0 0-2.8 4L2.8 11.3a1.3 1.3 0 0 0 1.9 1.9l4.8-4.9a3 3 0 0 0 4-2.8l-1.8 1-1.7-.8-.2-1.8Z",
  search: "M7 3a4 4 0 1 1 0 8 4 4 0 0 1 0-8ZM10 10l3.5 3.5",
  upload: "M8 10.5V2.5M5 5.5l3-3 3 3M2.5 10v3.5h11V10",
  download: "M8 2.5v8M5 7.5l3 3 3-3M2.5 10v3.5h11V10",
  reset: "M3 8a5 5 0 1 0 1.5-3.6M3 2.5V5h2.5",
  check: "M3 8.5 6.5 12 13 4.5",
  alert: "M8 2.5 14 13H2ZM8 6.5v3M8 11.2v.3",
  monitor: "M2 3h12v8H2ZM6 13.5h4M8 11v2.5",
};

export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden="true" className="icon">
      <path d={P[name]} fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
