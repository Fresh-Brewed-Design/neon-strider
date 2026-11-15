// Arithmetic for the calibration tools. A point's distance scale is the speed
// the treadmill reports divided by the speed actually walked. Rust applies the
// points; this only works them out.

/** Steps the user counts in one timed run. */
export const STEPS_PER_RUN = 50;

const kmh = (meters: number, seconds: number) => (seconds > 0 ? (meters / seconds) * 3.6 : 0);
const scaleFor = (reportedKmh: number, realKmh: number) => (realKmh > 0 ? reportedKmh / realKmh : 1.0);

/** Re-timing an existing point: `seconds` for STEPS_PER_RUN steps of `strideCm`. */
export function retimedPoint(strideCm: number, seconds: number, reportedKmh: number) {
  const realKmh = kmh((strideCm * STEPS_PER_RUN) / 100, seconds);
  return { realKmh, scale: scaleFor(reportedKmh, realKmh) };
}

export type WizardMode = "MANUAL" | "LOGO";

export interface WizardInput {
  mode: WizardMode;
  /** MANUAL: the stride the user picked. */
  strideCm: number;
  /** LOGO: how many times the belt logo went past, and the belt's length. */
  logoPasses: number;
  beltLengthCm: number;
  steps: number;
  elapsedSeconds: number;
  /** The raw (uncalibrated) speed the treadmill reported during the run. */
  reportedKmh: number;
}

/** The walked distance from the user's count, and the point it gives. */
export function wizardResult(input: WizardInput) {
  const distanceM =
    input.mode === "LOGO" ? (input.logoPasses * input.beltLengthCm) / 100 : (input.strideCm / 100) * input.steps;
  const realKmh = kmh(distanceM, input.elapsedSeconds);
  return {
    distanceM,
    scale: scaleFor(input.reportedKmh, realKmh),
    strideCm: (distanceM / input.steps) * 100,
  };
}
