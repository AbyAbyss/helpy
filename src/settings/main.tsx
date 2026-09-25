import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../styles/tokens.css";
import "./settings.css";
import { App } from "./App";
import { api } from "../lib/ipc";

// Native glass (macOS vibrancy, Windows Mica) shows through when it's on.
api.windowGlass().then((g) => g && document.documentElement.setAttribute("data-glass", g), () => {});
// macOS: no title bar; the traffic lights float over the sidebar.
if (navigator.platform.includes("Mac")) document.documentElement.setAttribute("data-mac", "");

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <div className="dragbar" data-tauri-drag-region />
    <App />
  </StrictMode>,
);
