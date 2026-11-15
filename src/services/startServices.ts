import { getCurrentWindow } from "@tauri-apps/api/window";
import { startSettingsSync } from "./controlOrchestrator";
import { startHistoryRefresh } from "./sessionCoordinator";
import { hydrateSessionMergeThreshold } from "./sessionMergeSetting";

/** Background work that only the main window runs. Call once at startup. */
export function startServices(): void {
  if (getCurrentWindow().label !== "main") return;
  startSettingsSync();
  startHistoryRefresh();
  void hydrateSessionMergeThreshold();
}
