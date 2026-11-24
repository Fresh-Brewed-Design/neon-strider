// FIT_TO_TIME (#258): the length picker beside each program's Run on the
// Programs page. No flag: every program whose length can change gets one, as
// in Rust. Running with a length starts a fitted copy; the saved program stays.
import type { FitToTimeOption, Program, ProgramStep } from "../../ipc";
import { snapshot } from "../mockState";
import { startProgram } from "./programs";
import type { Scenario } from "./types";

// Rust's `fit_to_time` limits, with their reasons beside them there.
const MIN_FIT = 5;
const MAX_FIT = 180;
const MIN_STEP = 1;
const MIN_SHRINK = 1 / 3;
const MAX_STRETCH = 3;
const EPS = 0.001;

const keeps = (s: ProgramStep) => s.kind === "warm_up" || s.kind === "cool_down";
const openEnd = (p: Program) => p.steps.length > 0 && p.steps[p.steps.length - 1].duration_minutes == null;
const timed = (p: Program) => p.steps.reduce((a, s) => a + (s.duration_minutes ?? 0), 0);
const fixed = (p: Program) => p.steps.filter(keeps).reduce((a, s) => a + (s.duration_minutes ?? 0), 0);
const scaled = (p: Program) => p.steps.filter((s) => !keeps(s) && (s.duration_minutes ?? 0) > 0).map((s) => s.duration_minutes ?? 0);

/** Rust's `fit_to_time::range`. */
function range(p: Program): [number, number] | null {
  let lo: number;
  let hi: number;
  if (openEnd(p)) {
    lo = Math.max(timed(p) + MIN_STEP, MIN_FIT);
    hi = MAX_FIT;
  } else {
    const mins = scaled(p);
    const total = mins.reduce((a, m) => a + m, 0);
    if (mins.length === 0 || total <= 0) return null;
    let loF = MIN_STEP / Math.min(...mins);
    let hiF = Infinity;
    if (mins.length >= 2) {
      loF = Math.max(loF, MIN_SHRINK);
      hiF = MAX_STRETCH;
    }
    lo = Math.max(fixed(p) + total * loF, MIN_FIT);
    hi = Math.min(fixed(p) + total * hiF, MAX_FIT);
  }
  const a = Math.ceil(lo - EPS);
  const b = Math.floor(hi + EPS);
  return a <= b ? [a, b] : null;
}

/** Rust's `fit_to_time::fit`, without its rounding to whole seconds. */
function fit(p: Program, minutes: number): Program {
  const r = range(p);
  if (!r) throw `${p.name} keeps every step's minutes, so its length can't change.`;
  if (minutes < r[0] || minutes > r[1]) throw `${p.name} runs from ${r[0]} to ${r[1]} min.`;
  let steps: ProgramStep[];
  if (openEnd(p)) {
    const left = minutes - timed(p);
    steps = p.steps.map((s) => (s.duration_minutes == null ? { ...s, duration_minutes: left } : { ...s }));
  } else {
    const factor = (minutes - fixed(p)) / scaled(p).reduce((a, m) => a + m, 0);
    steps = p.steps.map((s) =>
      keeps(s) || s.duration_minutes == null || s.duration_minutes <= 0 ? { ...s } : { ...s, duration_minutes: s.duration_minutes * factor },
    );
  }
  return { ...p, steps, repeat_every_minutes: null };
}

/** Rust's `fit_to_time::option`. */
function option(p: Program): FitToTimeOption | null {
  const r = range(p);
  if (!r) return null;
  const plain = !openEnd(p) && p.repeat_every_minutes == null;
  const saved = timed(p);
  const savedWhole = plain && Math.abs(saved - Math.round(saved)) < EPS ? Math.round(saved) : null;
  const lengths: number[] = [];
  for (const m of [r[0], ...Array.from({ length: r[1] - r[0] + 1 }, (_, i) => r[0] + i).filter((m) => m % 5 === 0), r[1]]) {
    if (m !== savedWhole && lengths[lengths.length - 1] !== m) lengths.push(m);
  }
  const has = (kind: ProgramStep["kind"]) => p.steps.some((s) => s.kind === kind && s.duration_minutes != null);
  const note: string[] = [];
  if (has("warm_up") && has("cool_down")) note.push("WARM_UP and COOL_DOWN keep their minutes.");
  else if (has("warm_up")) note.push("WARM_UP keeps its minutes.");
  else if (has("cool_down")) note.push("COOL_DOWN keeps its minutes.");
  if (p.steps.some((s) => s.pulse_end)) note.push("COOL_DOWN still walks on until the pulse is down.");
  if (p.repeat_every_minutes != null) note.push("The program runs once with no repeat.");
  note.push("The saved program stays as it is.");
  const label = Number.isInteger(saved) ? `${saved} min` : `${saved.toFixed(1)} min`;
  return { program_id: p.id, saved_label: plain ? label : "As saved", lengths, note: note.join("\n") };
}

const options = () => snapshot.programs.map(option).filter((o): o is FitToTimeOption => o !== null);

export const fitToTime: Scenario = {
  apply() {
    snapshot.fit_to_time = options();
  },
  tick() {
    const next = options();
    if (JSON.stringify(next) === JSON.stringify(snapshot.fit_to_time)) return false;
    snapshot.fit_to_time = next;
    return true;
  },
  commands: {
    program_run: (c) => {
      const p = snapshot.programs.find((x) => x.id === c.id);
      if (!p) throw "Program not found";
      snapshot.active_program = startProgram(c.minutes == null ? p : fit(p, c.minutes));
    },
  },
};
