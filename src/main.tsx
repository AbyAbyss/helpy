import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/bricolage-grotesque";
import "@fontsource-variable/instrument-sans";
import "@fontsource-variable/jetbrains-mono";
import "./design/tokens.css";
import { windowLabel } from "./lib/ipc";
import { OverlayApp } from "./overlay/OverlayApp";
import { SettingsApp } from "./settings/SettingsApp";

// One bundle, routed by window label: "overlay-<n>" or "settings".
const label = windowLabel();
const isOverlay = label.startsWith("overlay-");
if (isOverlay) document.documentElement.classList.add("is-overlay");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isOverlay ? <OverlayApp /> : <SettingsApp />}</React.StrictMode>,
);
