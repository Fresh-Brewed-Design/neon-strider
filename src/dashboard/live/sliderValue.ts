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

/** Where a value sits along the track, in percent, held to the ends. */
export function trackPct(value: number, min: number, max: number): number {
  return max > min ? Math.min(100, Math.max(0, ((value - min) / (max - min)) * 100)) : 0;
}

/**
 * The CSS left of a value's centre on the track. The 16 px thumb travels the
 * track minus its own width, so its centre runs from 8 px to the width less
 * 8 px; a mark placed with this lines up with the thumb's centre at that value.
 */
export function thumbCentre(pct: number): string {
  return `calc(${pct}% + ${8 - pct * 0.16}px)`;
}

/** A labelled mark on the track: STRIDE_FIT on the speed slider. */
export type SliderMark = { value: number; label: string; title: string };
