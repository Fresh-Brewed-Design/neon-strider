import { StrictMode } from "react";
import { MotionConfig } from "framer-motion";
import ReactDOM from "react-dom/client";
import { DashboardApp } from "./dashboard/DashboardApp";
import { startServices } from "./services/startServices";
import "./index.css";

startServices();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    {/* The dashboard's motion follows the OS's reduce motion setting, as the HUD's does. */}
    <MotionConfig reducedMotion="user">
      <DashboardApp />
    </MotionConfig>
  </StrictMode>,
);
