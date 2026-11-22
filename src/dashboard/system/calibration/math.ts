// The factor limits Rust applies to a stored calibration point. Rust applies
// the points; this only shows what it does with one outside the limits.

/** The factor limits Rust applies (`snapshot.calibration.limits`). */
export interface ScaleLimits {
  min_scale: number;
  max_scale: number;
}

/** The factor Rust applies for a stored one: the stored value held inside the limits. */
export const appliedScale = (scale: number, limits: ScaleLimits) =>
  Math.min(limits.max_scale, Math.max(limits.min_scale, scale));
