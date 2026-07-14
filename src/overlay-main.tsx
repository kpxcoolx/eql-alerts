import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import Overlay from "./Overlay";
import "./Overlay.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Overlay />
  </StrictMode>
);
