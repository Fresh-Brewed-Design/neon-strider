// The guided calibration's rows. Rust does the math and keeps the target list
// (calibration.rs); this only lays out which saved point each row is.
import type { CalibrationGuideView, GuideCheck } from "../../../ipc";
import type { CalibrationPoint } from "../../../store/types";

export interface CalibrationRow {
  /** Deck km/h the run asks for. */
  targetKmh: number;
  /** The saved point at this speed, if any. */
  point: CalibrationPoint | null;
  /** Its index in the saved list, for "repeat this point". */
  index: number | null;
}

/** A saved point this close to a target is that target's point: half a deck step, as `calibration.rs` `SAME_SPEED_KMH`. */
const SAME_SPEED_KMH = 0.05;

/**
 * One row per offered speed, plus one per saved point at another speed, by
 * speed. A row with a point repeats it; one without measures a new one.
 */
export function calibrationRows(points: CalibrationPoint[], targets: number[]): CalibrationRow[] {
  const rows: CalibrationRow[] = points.map((point, index) => ({ targetKmh: point.speed, point, index }));
  for (const t of targets) {
    if (!rows.some((r) => Math.abs(r.targetKmh - t) <= SAME_SPEED_KMH)) rows.push({ targetKmh: t, point: null, index: null });
  }
  return rows.sort((a, b) => a.targetKmh - b.targetKmh);
}

/** The row an open run belongs to: the one nearest its target speed. */
export function runRowFor(rows: CalibrationRow[], targetKmh: number): CalibrationRow | null {
  let best: CalibrationRow | null = null;
  for (const r of rows) {
    const d = Math.abs(r.targetKmh - targetKmh);
    if (d <= SAME_SPEED_KMH && (!best || d < Math.abs(best.targetKmh - targetKmh))) best = r;
  }
  return best;
}

/** Seconds as m:ss.t for the run's clock. */
export function chrono(seconds: number): string {
  const tenths = Math.round(Math.max(0, seconds) * 10);
  const m = Math.floor(tenths / 600);
  const rest = ((tenths - m * 600) / 10).toFixed(1).padStart(4, "0");
  return `${m}:${rest}`;
}

/** The list entry for a row's speed, if the row comes from the stored list. */
export function listedTarget<T extends { kmh: number }>(targets: T[], kmh: number): T | null {
  return targets.find((t) => Math.abs(t.kmh - kmh) <= SAME_SPEED_KMH) ?? null;
}

/** A deck within this much of the belt reads as matching it: half a percent is under the deck's own 0.1 km/h step at a walk. */
const MATCH_PCT = 0.5;

/**
 * The plain-words result of a tap run: how far the deck's display is from the
 * belt at the run's speed. `real` and `reported` are km/h.
 * `status` ("saved") ends the sentence after a semicolon.
 */
export function deckVerdict(speedKmh: number, realKmh: number, reportedKmh: number, status?: string): string {
  const pct = (realKmh / reportedKmh - 1) * 100;
  const at = `At ${speedKmh.toFixed(1)} km/h`;
  const end = status ? `; ${status}.` : ".";
  if (!Number.isFinite(pct)) return `${at} the taps gave no speed${end}`;
  if (Math.abs(pct) < MATCH_PCT) return `${at} the deck matches the belt${end}`;
  const size = Math.abs(pct) < 10 ? Math.abs(pct).toFixed(1) : Math.abs(pct).toFixed(0);
  return `${at} the deck runs ${size} % ${pct < 0 ? "slow" : "fast"}${end}`;
}

/** The line under a tap run's counter about taps dropped against the median, if any. */
export function droppedLine(dropped: number): string | null {
  if (dropped <= 0) return null;
  return dropped === 1 ? "1 tap dropped as a miss or a double." : `${dropped} taps dropped as misses or doubles.`;
}

/** Speeds as plain words: "0.6, 2.8 and 5.0 km/h". */
export function speedList(kmh: number[]): string {
  const s = kmh.map((k) => k.toFixed(1));
  if (s.length <= 1) return `${s.join("")} km/h`;
  return `${s.slice(0, -1).join(", ")} and ${s[s.length - 1]} km/h`;
}

/** A percent for the walk's lines: one decimal. */
function pct(v: number): string {
  return `${v.toFixed(1)} %`;
}

/** One line per midpoint check of the guided walk, in plain words. */
export function checkLine(c: GuideCheck, tolerancePct: number): string {
  const span = `${c.lo_kmh.toFixed(1)} to ${c.hi_kmh.toFixed(1)} km/h`;
  const at = c.mid_kmh.toFixed(1);
  const off = c.error_pct === null ? "" : pct(c.error_pct);
  switch (c.verdict) {
    case "pending":
      return `${span}: the check at ${at} km/h waits for RUN.`;
    case "agreed":
      return `${span}: ${off} off at ${at} km/h, within ${pct(tolerancePct)}.`;
    case "split":
      return `${span}: ${off} off at ${at} km/h, so ${at} km/h became a point and each half gets a check.`;
    case "at_limit":
      return `${span}: ${off} off at ${at} km/h after the last check level.`;
    case "skipped":
      return `${span}: the check at ${at} km/h was skipped, so the error there is unknown.`;
  }
}

/** The walk's summary once every speed is done: what was calibrated, and how far off the curve still is. */
export function guideSummary(g: CalibrationGuideView): string[] {
  const saved = g.speeds.filter((s) => s.mark === "saved").map((s) => s.kmh);
  const lines = [saved.length === 0 ? "No speed was calibrated." : `Calibrated ${speedList(saved)}.`];
  const ended = g.checks.filter((c) => c.verdict === "agreed" || c.verdict === "at_limit");
  if (g.remaining_error_pct !== null) {
    const within = g.remaining_error_pct <= g.tolerance_pct;
    lines.push(
      within
        ? `Between the calibrated speeds, distance is within ${pct(g.remaining_error_pct)} of the belt.`
        : `Distance is still up to ${pct(g.remaining_error_pct)} off between some calibrated speeds.`,
    );
  } else if (saved.length >= 2 && g.checks.length === 0) {
    lines.push("The deck has no speed between the two calibrated speeds to check.");
  }
  if (g.checks.some((c) => c.verdict === "skipped")) lines.push("A skipped check leaves the distance error at its midpoint unknown.");
  if (ended.length > 0) lines.push("Each check run measured the error halfway between two calibrated speeds.");
  return lines;
}

/** Speeds a stepper may offer: the deck's own settings in 0.1 steps, from `min` to `max`. */
export function settingSteps(min: number, max: number): number[] {
  const out: number[] = [];
  for (let b = Math.ceil(Math.max(min, 0.1) * 10 - 1e-6); b <= Math.round(max * 10); b++) out.push(b / 10);
  return out;
}
