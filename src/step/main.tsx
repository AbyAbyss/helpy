import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../styles/tokens.css";
import "./step.css";
import { StepCard } from "./StepCard";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <StepCard />
  </StrictMode>,
);
