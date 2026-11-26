// VELOCITY's speed trace (#362). ?speedspark=steady is ten minutes of a steady
// 3.5 km/h walk, =dips the same walk with Neural flow easing off during a
// typing burst and a call's quiet speed later, =gap a minute with the link
// down and one reading in between.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Rust's window: 10 minutes at one point per 5 s. */
const POINTS = 120;

/** The belt's small wobble around its target, repeatable so screenshots match. */
const wobble = (i: number) => Math.sin(i * 0.5) * 0.04 + Math.sin(i * 0.13) * 0.03;

const round = (kmh: number) => Math.round(kmh * 100) / 100;

function steady(): (number | null)[] {
  return Array.from({ length: POINTS }, (_, i) => round(3.5 + wobble(i)));
}

/** Neural flow down to 2.9 km/h for two minutes, then a call at 2.0 km/h. */
function dips(): (number | null)[] {
  return Array.from({ length: POINTS }, (_, i) => {
    if (i >= 30 && i < 54) return round(2.9 + wobble(i));
    if (i >= 84 && i < 108) return round(2.0 + wobble(i) / 2);
    return round(3.5 + wobble(i));
  });
}

function gap(): (number | null)[] {
  const p = steady();
  for (let i = 60; i < 76; i++) p[i] = null;
  p[67] = 3.4;
  return p;
}

const SHAPES: Record<string, () => (number | null)[]> = { steady, dips, gap };

export const speedSpark: Scenario = {
  apply() {
    const shape = SHAPES[params.get("speedspark") ?? ""];
    if (!shape) return;
    const trace = shape();
    snapshot.speed_trace = trace;
    const last = trace[trace.length - 1];
    if (last !== null) snapshot.deck.speed_kmh = last;
  },
};
