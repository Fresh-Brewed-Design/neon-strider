// ?mac: the app running on macOS: its words ("this Mac", System Settings), its
// ⌃⌥ shortcuts, and the features it doesn't have yet hidden (slow down on
// calls, the minify key choice).
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const platform: Scenario = {
  apply() {
    if (!params.has("mac")) return;
    snapshot.platform = {
      os: "macos",
      computer: "Mac",
      settings: "System Settings",
      microphone: false,
      minify_key_choice: false,
      shortcuts: {
        speed_up: "⌃⌥=",
        speed_down: "⌃⌥-",
        incline_up: "⌃⌥↑",
        incline_down: "⌃⌥↓",
        toggle_start: "⌃⌥↩",
        toggle_program: "⌃⌥P",
        toggle_minified: "⌃⌥M",
        calibration_tap: "⌃⌥Space",
      },
    };
  },
};
