import type { WallClock } from "../../ipc";

/** The plain sentence under WALL_CLOCK. */
export function wallClockText(clock: WallClock): string {
  if (clock.desk_days === 0) return "No desk days recorded yet.";
  const days = clock.desk_days === 1 ? "your last desk day" : `your last ${clock.desk_days} desk days`;
  if (!clock.hours.some((n) => n > 0)) return `No walks on ${days}.`;
  return clock.desk_days === 1 ? `Lit hours had a walk on ${days}.` : `An hour glows brighter for each of ${days} with a walk in that hour.`;
}

/** `09:00`: the hour a cell starts. */
export function hourLabel(hour: number): string {
  return `${String(hour).padStart(2, "0")}:00`;
}
