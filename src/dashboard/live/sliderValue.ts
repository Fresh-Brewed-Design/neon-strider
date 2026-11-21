/**
 * The slider's value from its input, to 0.1 and kept inside [min, max]: the
 * rounding never takes it under the lowest setting (the speed slider's is the
 * belt's floor, so it never asks a moving belt for 0).
 */
export function sliderValue(raw: string, min: number, max: number): number {
  const v = Math.round(parseFloat(raw) * 10) / 10;
  if (!Number.isFinite(v)) return min;
  return Math.min(max, Math.max(min, v));
}
