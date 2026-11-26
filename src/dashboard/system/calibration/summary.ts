// The compact GROUND_TRUTH status: how many points, which speeds, and when the dashboard last saw them change.

import type { CalibrationPoint } from "../../../store/types";
import { readPref, writePref } from "../../localPrefs";

const MEASURED_KEY = "calibration.measured";
const DAY_MS = 86_400_000;

/** A short signature of a point set, so a stamp only counts for the points it was taken for. */
export function pointsSignature(points: readonly CalibrationPoint[]): string {
  return points.map((p) => `${p.speed}:${p.distance_scale}:${p.runs ?? 1}`).join("|");
}

/** Remembers that these points were just measured or changed. Per machine; a different set shows no date. */
export function stampMeasured(points: readonly CalibrationPoint[], now: number): void {
  writePref(MEASURED_KEY, JSON.stringify({ sig: pointsSignature(points), at: now }));
}

/** When these exact points were last seen changing, or null when this dashboard never saw them change. */
export function readMeasured(points: readonly CalibrationPoint[]): number | null {
  try {
    const saved = JSON.parse(readPref(MEASURED_KEY) ?? "null") as { sig?: string; at?: number } | null;
    return saved && saved.sig === pointsSignature(points) && Number.isFinite(saved.at) ? (saved.at as number) : null;
  } catch {
    return null;
  }
}

/** "today", "yesterday", "5 days ago", or the date past 60 days. */
export function ago(at: number, now: number): string {
  const days = Math.floor((startOfDay(now) - startOfDay(at)) / DAY_MS);
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  if (days <= 60) return `${days} days ago`;
  return new Date(at).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

function startOfDay(ms: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** the status line's parts: the count, the speed range and, when known, the day. */
export function summaryParts(points: readonly CalibrationPoint[], measuredAt: number | null, now: number): string[] {
  const speeds = points.map((p) => p.speed);
  const lo = Math.min(...speeds).toFixed(1);
  const hi = Math.max(...speeds).toFixed(1);
  return [
    points.length === 1 ? "1 point" : `${points.length} points`,
    lo === hi ? `${lo} km/h` : `${lo} to ${hi} km/h`,
    ...(measuredAt === null ? [] : [`Measured ${ago(measuredAt, now)}`]),
  ];
}
