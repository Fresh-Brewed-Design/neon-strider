// Distance, time and date text shared by the dashboard and the HUD.

/** Metres as km with two decimals, e.g. "1.72". */
export const km = (meters: number) => (meters / 1000).toFixed(2);

/** Lifetime distance: one decimal, whole km from 1000 up. */
export function lifetimeKm(distanceKm: number): string {
  return distanceKm >= 1000 ? Math.round(distanceKm).toString() : distanceKm.toFixed(1);
}

/** A running clock: mm:ss, or h:mm:ss from an hour up. */
export function formatClock(totalSeconds: number): string {
  const t = Math.max(0, Math.floor(totalSeconds));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const s = t % 60;
  const mm = m.toString().padStart(2, "0");
  const ss = s.toString().padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

/** m:ss for a countdown. */
export function mmss(totalSeconds: number): string {
  const s = Math.max(0, Math.round(totalSeconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** A length of time in words: "1h 05m", "12m" or "40s". */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  if (h > 0) return `${h}h ${String(m).padStart(2, "0")}m`;
  if (m > 0) return `${m}m`;
  return `${total}s`;
}

/** Local calendar day as YYYY-MM-DD. Not toISOString, which is UTC and shifts evenings to the next day. */
export function dayKey(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** "Today", "Yesterday" or a dated heading. Days are compared as local calendar dates, so a 23 or 25 hour day around a clock change does not shift the label. */
export function formatDayHeading(ms: number, nowMs: number = Date.now()): string {
  const now = new Date(nowMs);
  const today = dayKey(nowMs);
  const yesterday = dayKey(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1).getTime());
  const key = dayKey(ms);
  if (key === today) return "Today";
  if (key === yesterday) return "Yesterday";
  return new Date(ms).toLocaleDateString([], { weekday: "short", month: "short", day: "numeric", year: "numeric" });
}
