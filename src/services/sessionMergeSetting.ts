// The "join sessions closer than N minutes" setting. Rust reads the same key
// when it groups walks for Strava, so both sides join sessions the same way.

import { useTreadmillStore } from "../store/treadmillStore";
import { SettingsRepository } from "./db/repositories/settingsRepository";

const KEY = "session_merge_threshold_minutes";
export const MERGE_THRESHOLD_MIN = 1;
export const MERGE_THRESHOLD_MAX = 60;

function valid(v: unknown): v is number {
  return typeof v === "number" && Number.isFinite(v) && v >= MERGE_THRESHOLD_MIN && v <= MERGE_THRESHOLD_MAX;
}

/** Update the store and persist. Out-of-range values are clamped. */
export function saveSessionMergeThreshold(minutes: number): void {
  const next = Math.round(Math.min(MERGE_THRESHOLD_MAX, Math.max(MERGE_THRESHOLD_MIN, minutes)));
  useTreadmillStore.getState().setSessionMergeThreshold(next);
  SettingsRepository.set(KEY, next).catch((e) => console.error("[sessionMergeSetting] persist failed", e));
}

/** Load the saved threshold into the store at startup. A missing or bad value keeps the default. */
export async function hydrateSessionMergeThreshold(): Promise<void> {
  try {
    const saved = await SettingsRepository.get(KEY);
    if (valid(saved)) useTreadmillStore.getState().setSessionMergeThreshold(saved);
  } catch (e) {
    console.error("[sessionMergeSetting] hydrate failed", e);
  }
}
