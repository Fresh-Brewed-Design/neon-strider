// STEP_MAP (#234): the running program's whole plan, as rows for the Live
// page's list, a stepped speed and incline chart, and the HUD's next-step
// line. Pure functions of `ActiveProgram`, which carries the steps that run.

import type { ActiveProgram, HeartZone, ProgramStep } from "../../ipc";
import { STEP_KINDS } from "./format";

const num = (v: number) => (Number.isInteger(v) ? v.toString() : v.toFixed(1));

export type StepState = "done" | "current" | "upcoming";

/** One row of the step list: a step, or the rest between cycles of a repeating program. */
export interface PlanRow {
  key: string;
  /** 1-based step number; null for the rest between cycles. */
  number: number | null;
  /** The kind counted within its kind (`INTERVAL 3/5`, `WARM_UP`), `STEP 2` without one, or `CYCLE_REST`. */
  code: string;
  /** Null for an open-ended step, which runs until END. */
  minutes: number | null;
  /** Null for the rest between cycles, which goes back to the speed from before the program. */
  speedKmh: number | null;
  inclinePct: number | null;
  zone: HeartZone | null;
  state: StepState;
}

/** The code shown for a step: Rust's `StepKindSpot` label rule, or `STEP n` for a step with no kind. */
export function stepCode(steps: readonly ProgramStep[], index: number): string {
  const kind = steps[index]?.kind;
  if (!kind) return `STEP ${index + 1}`;
  const code = STEP_KINDS.find((k) => k.value === kind)?.code ?? kind.toUpperCase();
  const of = steps.filter((s) => s.kind === kind).length;
  if (of < 2) return code;
  const nth = steps.slice(0, index + 1).filter((s) => s.kind === kind).length;
  return `${code} ${nth}/${of}`;
}

/** Minutes of the timed steps; an open-ended step adds none. */
export function timedMinutes(steps: readonly ProgramStep[]): number {
  return steps.reduce((sum, s) => sum + (s.duration_minutes ?? 0), 0);
}

/**
 * The rest between cycles in minutes, as Rust's `phase_len_s` works it out:
 * the repeat minus the steps. Null when the program doesn't repeat, has an
 * open-ended step (so no cycle ends) or leaves no rest.
 */
export function cycleRestMinutes(steps: readonly ProgramStep[], repeat: number | null): number | null {
  if (repeat == null || steps.some((s) => s.duration_minutes == null)) return null;
  const rest = repeat - timedMinutes(steps);
  return rest > 0 ? rest : null;
}

function stepState(a: ActiveProgram, index: number): StepState {
  // While resting, every step of the cycle has run; step_index already points at the next cycle's first.
  if (a.phase === "resting") return "done";
  if (index < a.step_index) return "done";
  return index === a.step_index ? "current" : "upcoming";
}

/** The step list: each step in order, then the rest between cycles when the program repeats. */
export function planRows(a: ActiveProgram): PlanRow[] {
  const rows: PlanRow[] = a.steps.map((s, i) => ({
    key: `step-${i}`,
    number: i + 1,
    code: stepCode(a.steps, i),
    minutes: s.duration_minutes,
    speedKmh: s.speed_kmh,
    inclinePct: s.incline_pct,
    zone: s.zone ?? null,
    state: stepState(a, i),
  }));
  const rest = cycleRestMinutes(a.steps, a.repeat_every_minutes);
  if (rest != null) {
    rows.push({
      key: "rest",
      number: null,
      code: "CYCLE_REST",
      minutes: rest,
      speedKmh: null,
      inclinePct: null,
      zone: null,
      state: a.phase === "resting" ? "current" : "upcoming",
    });
  }
  return rows;
}

/** A row's time: `3 min`, or `until END` for an open-ended step. */
export function rowTime(minutes: number | null): string {
  return minutes == null ? "until END" : `${num(minutes)} min`;
}

/** A heart zone as % of max, e.g. `HR 85-95%`. */
export function zoneText(z: HeartZone): string {
  return `HR ${num(z.low_pct)}-${num(z.high_pct)}%`;
}

/**
 * The HUD's line under the running step, e.g. `NEXT // REST 2/4 · 3 min · 3.5 km/h`
 * (the incline only when it isn't flat). Null while the program waits for GO
 * (the panel already names the step that waits) and on an open-ended step,
 * which has no end to be followed by anything.
 */
export function nextStepLine(a: ActiveProgram): string | null {
  if (a.wait || a.steps.length === 0) return null;
  const describe = (index: number) => {
    const s = a.steps[index];
    const tilt = Math.abs(s.incline_pct) >= 0.05 ? ` · ${num(s.incline_pct)}%` : "";
    return `NEXT // ${stepCode(a.steps, index)} · ${rowTime(s.duration_minutes)} · ${s.speed_kmh.toFixed(1)} km/h${tilt}`;
  };
  if (a.phase === "resting") return describe(0);
  const current = a.steps[a.step_index];
  if (!current || current.duration_minutes == null) return null;
  if (a.step_index + 1 < a.steps.length) return describe(a.step_index + 1);
  if (a.repeat_every_minutes == null) return "NEXT // PROGRAM_END";
  const rest = cycleRestMinutes(a.steps, a.repeat_every_minutes);
  return rest != null ? `NEXT // CYCLE_REST · ${num(rest)} min` : describe(0);
}

// ---- Chart geometry ----

/**
 * The width an open-ended step gets on the chart, in minutes. It runs until
 * END, so no width is true to it: a quarter of the timed steps keeps it
 * visible beside them without squeezing them, and 5 minutes is the floor so a
 * program that is only an open step still fills the chart.
 */
export function openStepMinutes(timed: number): number {
  return Math.max(5, timed / 4);
}

export interface ChartBlock {
  key: string;
  x0: number;
  x1: number;
  /** Top of the speed area; null for the rest between cycles. */
  speedY: number | null;
  inclineY: number | null;
  rest: boolean;
  open: boolean;
  state: StepState;
}

export interface StepChart {
  blocks: ChartBlock[];
  /** Stepped outlines across the steps (the rest between cycles is left out). */
  speedPath: string;
  inclinePath: string;
  /** The "you are here" line. */
  hereX: number;
  /** Top of each axis, with headroom above the highest step. */
  maxSpeed: number;
  maxIncline: number;
  /** Minutes along the chart, the open step's stand-in width included. */
  spanMinutes: number;
}

/**
 * Where the program is, in chart minutes: the start of the running step plus
 * the time it has run (from `seconds_remaining`), or into the rest between
 * cycles. An open-ended step has no remaining time to count from, so the line
 * stays at its start.
 */
function hereMinutes(a: ActiveProgram, starts: number[], timed: number, rest: number | null): number {
  if (a.phase === "resting") {
    if (rest == null) return timed;
    const left = (a.seconds_remaining ?? rest * 60) / 60;
    return timed + Math.min(rest, Math.max(0, rest - left));
  }
  const step = a.steps[a.step_index];
  const start = starts[a.step_index] ?? 0;
  if (!step || step.duration_minutes == null || a.seconds_remaining == null) return start;
  const ran = step.duration_minutes - a.seconds_remaining / 60;
  return start + Math.min(step.duration_minutes, Math.max(0, ran));
}

/** The chart in a `width` by `height` box: x is minutes, y counts down from the top. */
export function stepChart(a: ActiveProgram, width: number, height: number): StepChart {
  const timed = timedMinutes(a.steps);
  const openWidth = openStepMinutes(timed);
  const lengths = a.steps.map((s) => s.duration_minutes ?? openWidth);
  const starts: number[] = [];
  let t = 0;
  for (const len of lengths) {
    starts.push(t);
    t += len;
  }
  const rest = cycleRestMinutes(a.steps, a.repeat_every_minutes);
  const span = Math.max(t + (rest ?? 0), 1e-6);
  const maxSpeed = Math.max(4, ...a.steps.map((s) => s.speed_kmh)) * 1.1;
  // More headroom than speed (History's 1.1) so a climb's incline line sits
  // under its speed line instead of on top of it.
  const maxIncline = Math.max(5, ...a.steps.map((s) => s.incline_pct)) * 1.6;
  const x = (m: number) => (m / span) * width;
  const y = (v: number, max: number) => height - (Math.max(0, v) / max) * height;

  const blocks: ChartBlock[] = a.steps.map((s, i) => ({
    key: `step-${i}`,
    x0: x(starts[i]),
    x1: x(starts[i] + lengths[i]),
    speedY: y(s.speed_kmh, maxSpeed),
    inclineY: y(s.incline_pct, maxIncline),
    rest: false,
    open: s.duration_minutes == null,
    state: stepState(a, i),
  }));
  if (rest != null) {
    blocks.push({
      key: "rest",
      x0: x(t),
      x1: width,
      speedY: null,
      inclineY: null,
      rest: true,
      open: false,
      state: a.phase === "resting" ? "current" : "upcoming",
    });
  }

  const stepped = (pick: (b: ChartBlock) => number | null) => {
    const parts: string[] = [];
    for (const b of blocks) {
      const v = pick(b);
      if (v == null) continue;
      parts.push(`${parts.length === 0 ? "M" : "L"}${round(b.x0)} ${round(v)}`, `L${round(b.x1)} ${round(v)}`);
    }
    return parts.join(" ");
  };

  return {
    blocks,
    speedPath: stepped((b) => b.speedY),
    inclinePath: stepped((b) => b.inclineY),
    hereX: x(hereMinutes(a, starts, t, rest)),
    maxSpeed,
    maxIncline,
    spanMinutes: span,
  };
}

const round = (v: number) => Math.round(v * 100) / 100;
