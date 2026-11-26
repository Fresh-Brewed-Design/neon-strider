// The compact GROUND_TRUTH status: how many points and which speeds they cover.

import type { CalibrationPoint } from "../../../store/types";

/** The status line's parts: the point count and the speed range. */
export function summaryParts(points: readonly CalibrationPoint[]): string[] {
  const speeds = points.map((p) => p.speed);
  const lo = Math.min(...speeds).toFixed(1);
  const hi = Math.max(...speeds).toFixed(1);
  return [points.length === 1 ? "1 point" : `${points.length} points`, lo === hi ? `${lo} km/h` : `${lo} to ${hi} km/h`];
}
