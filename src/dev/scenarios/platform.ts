// ?mac: the app running on macOS: its words ("this Mac", System Settings), its
// ⌃⌥ shortcuts (scenarios/shortcuts.ts), and System -> Permissions with
// Bluetooth allowed and Input Monitoring not asked yet; `=denied` with Input
// Monitoring turned off.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const platform: Scenario = {
  apply() {
    if (!params.has("mac")) return;
    snapshot.platform = {
      os: "macos",
      computer: "Mac",
      settings: "System Settings",
      microphone: true,
      permissions: [
        { kind: "bluetooth", state: "allowed" },
        { kind: "input_monitoring", state: params.get("mac") === "denied" ? "denied" : "not_asked" },
      ],
    };
  },
  commands: {
    open_permission_settings: () => {},
  },
};
