import { SETTING_LIMITS } from "../../ipc";

const L = SETTING_LIMITS;

/** Rounds to 0.01 km/h, as the speed fields do. */
const r2 = (v: number) => Math.round(v * 100) / 100;

export interface Band {
  low: number;
  high: number;
}

/**
 * The band after one end was typed: the other end follows when the band
 * would be narrower or wider than Rust allows (`speed_drift_width_kmh`),
 * and both stay inside `speed_drift_kmh`. So a typed speed is always saved,
 * where a refused band would leave the field reverting.
 */
export function bandWith(band: Band, end: keyof Band, kmh: number): Band {
  const { min: lo, max: hi } = L.speed_drift_kmh;
  const { min: wMin, max: wMax } = L.speed_drift_width_kmh;
  if (end === "low") {
    const low = Math.min(Math.max(kmh, lo), hi - wMin);
    const high = Math.min(Math.max(band.high, low + wMin), low + wMax, hi);
    return { low: r2(low), high: r2(high) };
  }
  const high = Math.max(Math.min(kmh, hi), lo + wMin);
  const low = Math.max(Math.min(band.low, high - wMin), high - wMax, lo);
  return { low: r2(low), high: r2(high) };
}
