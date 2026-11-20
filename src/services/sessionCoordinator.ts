// Keeps the History list current: it reloads when a session starts or ends,
// once a minute while one is open, and when Rust reports a new Strava result.

import type { AppSnapshot } from "../ipc";
import { subscribeAppSnapshot } from "../state/useAppState";
import { useTreadmillStore } from "../store/treadmillStore";

/** History's open walk changes slowly; once a minute keeps it current without reloading every row each tick. */
const REFRESH_WHILE_ACTIVE_MS = 60_000;
/** Rust writes a closed session's final totals off-thread, so wait a moment before reloading (Strava's worker waits 3 s). */
const AFTER_SESSION_CHANGE_MS = 1500;

let started = false;
let lastRefreshAt = 0;
let lastSessionId: number | null = null;
// undefined until the first snapshot, so a result that is already there is not news.
let lastStravaResultAt: number | null | undefined = undefined;

function onSnapshot(snapshot: AppSnapshot) {
  // Only the History page uses this, and a hidden window shows no page.
  if (document.hidden) return;
  const { triggerHistoryRefresh } = useTreadmillStore.getState();

  const sessionId = snapshot.session.active_id;
  if (sessionId !== lastSessionId) {
    lastSessionId = sessionId;
    setTimeout(() => useTreadmillStore.getState().triggerHistoryRefresh(), AFTER_SESSION_CHANGE_MS);
  }

  // An upload, manual or background, marks sessions as on Strava.
  const resultAt = snapshot.strava.last_result?.at_ms ?? null;
  if (lastStravaResultAt !== undefined && resultAt !== lastStravaResultAt) triggerHistoryRefresh();
  lastStravaResultAt = resultAt;

  if (sessionId !== null) {
    const now = Date.now();
    if (now - lastRefreshAt >= REFRESH_WHILE_ACTIVE_MS) {
      triggerHistoryRefresh();
      lastRefreshAt = now;
    }
  }
}

export function startHistoryRefresh(): void {
  if (started) return;
  started = true;
  subscribeAppSnapshot(onSnapshot);
}
