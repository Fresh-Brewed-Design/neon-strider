// PULSE_RATE's spark chart. ?hrspark=steady is ten minutes of a steady walk,
// =climb a climb into a heart rate zone step (its band shows), =gap a sensor
// that dropped out for a minute and came back for one reading in between.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Rust's window: 10 minutes at one point per 5 s. */
const POINTS = 120;

/** A small, repeatable wobble, so screenshots match from run to run. */
const wobble = (i: number) => Math.round(Math.sin(i * 0.35) * 1.5 + Math.sin(i * 0.08) * 2);

function steady(): (number | null)[] {
  return Array.from({ length: POINTS }, (_, i) => 108 + wobble(i));
}

/** Flat, then rising into 133 to 152 bpm (70 to 80 % of 190) and holding. */
function climb(): (number | null)[] {
  return Array.from({ length: POINTS }, (_, i) => {
    const rise = Math.min(Math.max(i - 40, 0), 50) / 50;
    return Math.round(104 + rise * 40 + wobble(i));
  });
}

function gap(): (number | null)[] {
  const p = steady();
  for (let i = 60; i < 76; i++) p[i] = null;
  p[67] = 111;
  return p;
}

const SHAPES: Record<string, () => (number | null)[]> = { steady, climb, gap };

export const hrSpark: Scenario = {
  apply() {
    const shape = SHAPES[params.get("hrspark") ?? ""];
    if (!shape) return;
    // Pinned on the HUD after DAILY_TOTAL, so it pairs up in the stat grid.
    const w = snapshot.hud.widgets;
    if (!w.includes("HEART_RATE")) w.splice(w.indexOf("DAILY") + 1, 0, "HEART_RATE");
    const history = shape();
    const last = history[history.length - 1];
    Object.assign(snapshot.heart_rate, {
      sensor: { id: "AA:BB:CC:DD:EE:01", name: "Forerunner 265" },
      status: "connected",
      label: "LINKED",
      message: "Reading heart rate from the sensor.",
      bpm: last,
      history,
      zone: params.get("hrspark") === "climb" ? { low_bpm: 133, high_bpm: 152 } : null,
    });
  },
};
