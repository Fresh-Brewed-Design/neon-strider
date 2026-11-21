import type { ZoneBand } from "../../ipc";
import { ZONE_ACCENT, zoneRange } from "../../widgets/zone";
import { cx, TONE_TEXT } from "../util";

/**
 * PULSE_RATE's heart rate zone on the Live page, beside the stat's name:
 * Rust's word for under, in or over, then the range. Rust's sentence is the
 * tooltip, and screen readers read it.
 */
export function ZoneReadout({ zone }: { zone: ZoneBand }) {
  const tone = zone.position ? TONE_TEXT[ZONE_ACCENT[zone.position]] : "text-white/50";
  return (
    <span
      className="ml-auto min-w-0 text-right font-mono text-[11px] leading-snug tracking-[0.08em] [overflow-wrap:anywhere]"
      title={zone.message}
      data-testid="live-zone"
    >
      <span className={cx("font-bold", tone)}>{zone.label}</span>
      <span className="ml-2 text-white/60">
        {zoneRange(zone)} <span className="text-white/40">BPM</span>
      </span>
      <span className="sr-only">{zone.message}</span>
    </span>
  );
}
