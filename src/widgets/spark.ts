import type { ZoneBand } from "../ipc";

/** What a spark chart draws: Rust's points, oldest first, `null` for a gap. */
export interface SparkData {
  points: (number | null)[];
  /** A heart rate zone shown as a band behind the line. */
  zone: ZoneBand | null;
  /** A cool-down's pulse target (bpm), shown as a band from the target down. */
  target?: number | null;
}

export interface SparkGeometry {
  /** SVG path data; one subpath per run of readings, so a gap stays a gap. */
  line: string;
  /** The zone's band (or the band under a cool-down target), top edge and height, in the same units. */
  band: { y: number; height: number } | null;
}

/**
 * A steady walk drifts by a few bpm. Stretched to the full height that drift
 * would look like a climb, so the scale always spans at least 20 bpm.
 */
export const MIN_SPAN_BPM = 20;

/** Kept clear at the top and bottom so the stroke is never cut in half. */
const EDGE = 1.5;

const fmt = (n: number) => String(Math.round(n * 100) / 100);

/**
 * The line and band for `data` in a `width` x `height` box, newest point at
 * the right edge. No points (no sensor picked) is no chart: `null`.
 */
export function sparkGeometry(data: SparkData, width: number, height: number): SparkGeometry | null {
  const { points, zone } = data;
  const target = data.target ?? null;
  if (points.length === 0) return null;
  const values = points.filter((p): p is number => p !== null);
  const bounds = zone ? [...values, zone.low_bpm, zone.high_bpm] : target !== null ? [...values, target] : values;
  let lo = Math.min(...bounds);
  let hi = Math.max(...bounds);
  if (bounds.length === 0) {
    lo = 0;
    hi = MIN_SPAN_BPM;
  }
  if (hi - lo < MIN_SPAN_BPM) {
    const mid = (hi + lo) / 2;
    lo = mid - MIN_SPAN_BPM / 2;
    hi = mid + MIN_SPAN_BPM / 2;
  }
  const inner = height - EDGE * 2;
  const y = (bpm: number) => EDGE + ((hi - bpm) / (hi - lo)) * inner;
  const step = points.length > 1 ? width / (points.length - 1) : 0;
  const x = (i: number) => (points.length > 1 ? i * step : width);

  const parts: string[] = [];
  let inRun = false;
  points.forEach((p, i) => {
    if (p === null) {
      inRun = false;
      return;
    }
    const next = points[i + 1];
    const alone = !inRun && (next === undefined || next === null);
    // A lone reading between gaps is a dot: a zero-length stroke with a round cap.
    parts.push(`${inRun ? "L" : "M"}${fmt(x(i))} ${fmt(y(p))}${alone ? " h0" : ""}`);
    inRun = true;
  });

  const band = zone
    ? { y: y(zone.high_bpm), height: y(zone.low_bpm) - y(zone.high_bpm) }
    : target !== null
      ? { y: y(target), height: height - y(target) }
      : null;
  return { line: parts.join(" "), band };
}
