// Dev-only mocked backend shared by the HUD and dashboard previews
// (hud-preview.html, dashboard-preview.html). Not part of the production build.
// URL flags pick a scenario; each feature's flags, commands and queries are in
// its own file under `scenarios/`, registered in `scenarios/index.ts`.
// ?reject makes every command fail.
import { mockIPC } from "@tauri-apps/api/mocks";
import type { Command } from "../ipc";
import { applyCardQueue } from "./cardQueue";
import { PREVIEW_CONFIG, hudView, params, publish, snapshot, updateDot } from "./mockState";
import { SCENARIOS } from "./scenarios";
import type { CommandHandlers, Query } from "./scenarios/types";

export { snapshot } from "./mockState";
export { previewSettings } from "./scenarios/calibration";

for (const scenario of SCENARIOS) scenario.apply?.();

const rejecting = params.has("reject");

/** What each dispatched command does to the snapshot. Commands not listed only republish it. */
const COMMANDS: CommandHandlers = Object.assign({}, ...SCENARIOS.map((s) => s.commands));

/** The app's named `invoke` calls besides `dispatch`. */
const QUERIES: Record<string, Query> = {
  get_snapshot: () => {
    snapshot.hud_view = hudView();
    applyCardQueue(snapshot);
    snapshot.update.dot = updateDot();
    // A copy, as Rust's is: the window keeps what it got, so it must not change underneath it.
    return structuredClone(snapshot);
  },
  open_log_folder: () => null,
  get_rust_config: () => PREVIEW_CONFIG,
  ...Object.assign({}, ...SCENARIOS.map((s) => s.queries)),
  dispatch: (args) => {
    if (rejecting) throw "Treadmill not connected";
    const command = args.command as Command;
    (COMMANDS[command.type] as ((c: Command) => void) | undefined)?.(command);
    publish();
    return null;
  },
};

/** Answers the app's `invoke` calls and applies dispatched commands to the snapshot. */
export function installMockBackend() {
  for (const scenario of SCENARIOS) scenario.install?.();
  setInterval(() => {
    const ticked = SCENARIOS.map((s) => s.tick?.() ?? false);
    const viewChanged = hudView() !== snapshot.hud_view;
    const before = { card: snapshot.hud_card, folded: snapshot.seated.folded };
    const cardChanged = applyCardQueue(snapshot) !== before.card || snapshot.seated.folded !== before.folded;
    if (ticked.includes(true) || viewChanged || cardChanged) publish();
  }, 1000);
  mockIPC((cmd, payload) => QUERIES[cmd]?.((payload ?? {}) as Record<string, unknown>) ?? null, { shouldMockEvents: true });
}
