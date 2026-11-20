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

/** A point worth saving: a real speed and a positive scale. */
export const isSavablePoint = (speed: number, scale: number) =>
  Number.isFinite(speed) && Number.isFinite(scale) && speed > 0 && scale > 0;

/** The factor limits Rust applies (`snapshot.calibration.limits`). */
export interface ScaleLimits {
  min_scale: number;
  max_scale: number;
}

const inRange = (scale: number, l: ScaleLimits) => scale >= l.min_scale && scale <= l.max_scale;

/** Why a factor can't be saved, or null when it is inside the limits (or the limits are not known yet). */
export function scaleRefusal(scale: number, limits: ScaleLimits | null | undefined, advice: string): string | null {
  if (!limits || !Number.isFinite(scale) || inRange(scale, limits)) return null;
  return `Factor ${scale.toFixed(2)} is outside ${limits.min_scale.toFixed(1)} to ${limits.max_scale.toFixed(1)}. ${advice}`;
}

/** The factor Rust applies for a stored one: the stored value held inside the limits. */
export const appliedScale = (scale: number, limits: ScaleLimits) =>
  Math.min(limits.max_scale, Math.max(limits.min_scale, scale));
