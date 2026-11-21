import type { ZoneBand, ZonePosition } from "../ipc";
import type { Accent } from "./registry";

/** A heart rate zone's range for display, e.g. "133 to 152". */
export function zoneRange(z: ZoneBand): string {
  return `${z.low_bpm} to ${z.high_bpm}`;
}

/** The colour of each position Rust sends: under cool, in green, over warm. */
export const ZONE_ACCENT: Record<ZonePosition, Accent> = {
  under: "cyan",
  in: "green",
  over: "yellow",
};
