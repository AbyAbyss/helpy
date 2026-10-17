import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../styles/tokens.css";
import "./page.css";
import "./island.css";
import { Island } from "./Island";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Island />
  </StrictMode>,
);
