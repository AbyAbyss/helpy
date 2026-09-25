import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../styles/tokens.css";
import "./plan.css";
import { PlanCard } from "./PlanCard";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <PlanCard />
  </StrictMode>,
);
