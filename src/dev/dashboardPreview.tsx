// Dev-only dashboard preview (dashboard-preview.html) on the mocked backend.
import { StrictMode } from "react";
import ReactDOM from "react-dom/client";
import { installMockBackend, previewSettings } from "./mockBackend";
import { applyTextScale } from "./textScale";
import "../index.css";

installMockBackend();
applyTextScale();

const { DashboardApp } = await import("../dashboard/DashboardApp");
const { useTreadmillStore } = await import("../store/treadmillStore");
const seed = previewSettings();
if (seed) useTreadmillStore.getState().updateSettings(seed);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <DashboardApp />
  </StrictMode>,
);
