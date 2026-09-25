import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../styles/tokens.css";
import "./dock.css";
import { DockCard } from "./DockCard";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <DockCard />
  </StrictMode>,
);
