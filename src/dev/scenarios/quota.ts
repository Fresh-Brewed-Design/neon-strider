// ?quota=5: DAILY_QUOTA set to a 5 km daily target in System, Desk, so
// DAILY_TOTAL (the Live tile, the HUD's TODAY, STEP_MAP's readout) shows
// today's km against it with its bar. Any km from the limits works; with
// ?daily=7.25 today's km is past the target and the bar stays full.
// The share mirrors `daily_quota::quota` in Rust.
import type { DailyQuotaSnapshot } from "../../ipc";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

function quotaOf(target: number | null, dailyM: number): DailyQuotaSnapshot {
  if (target === null || target <= 0) return { target_km: null, fraction: 0, left_km: 0, message: "" };
  const walked = Math.max(0, dailyM) / 1000;
  const left = Math.round(Math.max(0, target - walked) * 100) / 100;
  return {
    target_km: target,
    fraction: left <= 0 ? 1 : Math.min(1, Math.max(0, walked / target)),
    left_km: left,
    message:
      left <= 0
        ? `Today's ${target.toFixed(1)} km target is walked.`
        : `${left.toFixed(2)} km left of today's ${target.toFixed(1)} km target.`,
  };
}

function refresh(): boolean {
  const next = quotaOf(snapshot.daily_quota.target_km, snapshot.session.daily_distance_m);
  const changed = JSON.stringify(next) !== JSON.stringify(snapshot.daily_quota);
  snapshot.daily_quota = next;
  return changed;
}

export const quota: Scenario = {
  apply() {
    const flag = Number(params.get("quota") || 5);
    if (params.has("quota") && Number.isFinite(flag)) snapshot.daily_quota.target_km = flag;
    refresh();
  },
  // Today's km grows while the mock walks, so the bar follows it.
  tick: refresh,
  commands: {
    daily_quota_set: (c) => {
      snapshot.daily_quota.target_km = c.km ?? null;
      refresh();
    },
  },
};
