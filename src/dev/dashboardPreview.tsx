// Dev-only dashboard preview (dashboard-preview.html) on the mocked backend.
import { StrictMode } from "react";
import ReactDOM from "react-dom/client";
import { installMockBackend } from "./mockBackend";
import "../index.css";

installMockBackend();

const { DashboardApp } = await import("../dashboard/DashboardApp");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <DashboardApp />
  </StrictMode>,
);
