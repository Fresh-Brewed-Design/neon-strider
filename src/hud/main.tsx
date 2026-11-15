import { StrictMode } from "react";
import ReactDOM from "react-dom/client";
import { HudApp } from "./HudApp";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <HudApp />
  </StrictMode>,
);
