import { StrictMode } from "react";
import ReactDOM from "react-dom/client";
import { DashboardApp } from "./dashboard/DashboardApp";
import { startServices } from "./services/startServices";
import "./index.css";

startServices();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <DashboardApp />
  </StrictMode>,
);
