// DAILY_UPLINK's hook and text, apart from its component (fast refresh).
import { useBackendAction, type BackendAction } from "../components/useAction";
import type { StravaSnapshot } from "../ipc";

/** DAILY_UPLINK's own pending and result, from Rust's last Strava result. */
export function useDailyUplink(strava: StravaSnapshot | null): BackendAction {
  return useBackendAction(strava?.last_result ?? null, strava?.syncing ?? false);
}

/** "3 walks · 4.20 km as one activity" while there is something to send, else Rust's reason. */
export function dailyUplinkLine(today: StravaSnapshot["today"]): string {
  if (today.blocked !== null) return today.blocked;
  const walks = `${today.walks} walk${today.walks === 1 ? "" : "s"}`;
  return `${walks} · ${today.distance_km.toFixed(2)} km go up as one activity.`;
}
