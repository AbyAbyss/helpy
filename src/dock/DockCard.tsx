import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, EVENTS } from "../lib/ipc";
import { useSettings, useTheme } from "../lib/useSettings";
import { Card } from "./Card";
import { useAgents } from "./Dock";

/**
 * The card that slides out of the dock, in a window of its own so native
 * glass (macOS vibrancy, Windows Acrylic) can fill exactly its shape. Rust
 * places it beside the hovered chip from the size reported here.
 */
export function DockCard() {
  const [settings] = useSettings();
  useTheme(settings);
  const { agents, live } = useAgents();
  const [open, setOpen] = useState<string | null>(null);
  const [glass, setGlass] = useState<string | null>(null);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.windowGlass().then(setGlass, () => {});
    api.dockCardCurrent().then(setOpen, () => {});
    const off = listen<string | null>(EVENTS.dockCard, (e) => setOpen(e.payload));
    return () => void off.then((f) => f());
  }, []);

  const agent = open ? agents.get(open) : undefined;

  // The window is exactly the card's size.
  useEffect(() => {
    const el = root.current;
    if (!el || !agent) return;
    const report = () => {
      const r = el.getBoundingClientRect();
      api.dockCardLayout(Math.ceil(r.width), Math.ceil(r.height)).catch(() => {});
    };
    const ro = new ResizeObserver(report);
    ro.observe(el);
    report();
    return () => ro.disconnect();
  }, [agent?.id]);

  return (
    <div
      ref={root}
      className="cardwin"
      data-glass={glass ?? undefined}
      data-side={settings?.agents.dockSide ?? "right"}
      onMouseEnter={() => api.dockCardHover("card", true)}
      onMouseLeave={() => api.dockCardHover("card", false)}
    >
      {agent && (
        <Card
          key={agent.id}
          agent={agent}
          line={live.get(agent.id)}
          merged={!!glass}
          onPin={(on) => api.dockCardPin(on)}
          onClose={() => api.dockCardClose()}
        />
      )}
    </div>
  );
}
