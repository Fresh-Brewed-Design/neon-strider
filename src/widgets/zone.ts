import type { PulseTarget, ZoneBand, ZonePosition } from "../ipc";
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

/** A cool-down's pulse target for display, e.g. "to 114". */
export function targetRange(t: PulseTarget): string {
  return `to ${t.target_bpm}`;
}

/** The colour of a pulse against a cool-down target: down green, still high warm, no reading none. */
export function targetAccent(t: PulseTarget): Accent | null {
  if (t.down === null) return null;
  return t.down ? "green" : "yellow";
}
