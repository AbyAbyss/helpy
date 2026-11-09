import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../styles/tokens.css";
import "../agents/panel.css";
import "./chats.css";
import { Chats } from "./Chats";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Chats />
  </StrictMode>,
);
