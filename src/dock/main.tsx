import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../styles/tokens.css";
import "./dock.css";
import { Dock } from "./Dock";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Dock />
  </StrictMode>,
);
