// Pure helpers for the update UI: what the badge, the buttons and the install
// confirmation say. Tested in updates.test.ts.

import type { AppSnapshot, UpdateSnapshot } from "../../../ipc";

/**
 * The pink "update waiting" dot on the System nav and the Updates tab. Rust
 * decides it (`update::show_update_dot`, which the tray icon follows too), so
 * all three show and clear together.
 */
export function showUpdateBadge(u: UpdateSnapshot | undefined): boolean {
  return !!u?.dot;
}

/** Rust is checking, downloading or installing. */
export function updateBusy(u: UpdateSnapshot): boolean {
  return u.status.state === "checking" || u.status.state === "downloading" || u.status.state === "installing";
}

/** The spinner text while an install runs, or null. */
export function installProgress(u: UpdateSnapshot): string | null {
  const s = u.status;
  if (s.state === "downloading") {
    return s.percent === null ? "Downloading…" : `Downloading ${s.percent}%…`;
  }
  if (s.state === "installing") return "Installing: Neon Strider closes and opens again by itself.";
  return null;
}

/**
 * The install confirmation, true to what Rust does (update.rs
 * `save_running_program`): nothing is sent to the treadmill, so a moving belt
 * keeps going by itself while the app is closed, and the walk (or the program,
 * at the same step) carries on after the reconnect. The app never starts a
 * still belt by itself after a restart, so a resting program's next step waits
 * for GO. The message is one sentence naming the update; the hint carries the detail.
 */
export function installConfirmText(s: AppSnapshot, version: string): { message: string; hint?: string } {
  const message = `Neon Strider closes, installs v${version} and opens again, usually in under a minute.`;
  const moving = s.deck.status === "Running" || s.deck.status === "Starting";
  const program = s.active_program;
  const resting = program?.phase === "resting";
  if (!moving) {
    return resting ? { message, hint: `${program.name} keeps resting and the next step waits for GO on the HUD.` } : { message };
  }
  const carriesOn =
    program && !resting
      ? `The belt keeps going and ${program.name} carries on at the same step.`
      : `The belt keeps going and the walk carries on as the same session${resting ? `, with ${program.name} still resting` : ""}.`;
  return { message, hint: `${carriesOn} The HUD and shortcuts stop working for a few seconds, so use the treadmill's own controls.` };
}

/** "14 Nov 2026" for an RFC 3339 time, or null. */
export function formatPublished(iso: string | null): string | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  return new Date(t).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}
