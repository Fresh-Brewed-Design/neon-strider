import { useState } from "react";
import type { ActiveProgram } from "../ipc";

/**
 * The HUD's one-shot motion: LINK UP (the title decodes when the treadmill
 * connects, #374) and the step sweep (a scanline with the new step's name,
 * #376). Display only: nothing here sends a command. Both stay off while the
 * OS asks for reduced motion, and neither ever loops (hud.css).
 */
export function prefersReducedMotion(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * Counts connects: moves by one each time the link goes from down to up. The
 * first snapshot only sets the baseline, so opening or reloading the HUD with
 * the link already up never plays it, and the 2 s heartbeat (same `connected`)
 * never plays it again. 0 until the first connect.
 */
export function useLinkUp(connected: boolean | undefined): number {
  const [prev, setPrev] = useState<boolean | undefined>(undefined);
  const [count, setCount] = useState(0);
  if (connected !== undefined && connected !== prev) {
    // Adjusting state while rendering: React re-renders before painting.
    setPrev(connected);
    if (prev === false && connected && !prefersReducedMotion()) setCount(count + 1);
  }
  return count;
}

/** A step sweep to draw: `key` moves by one for each, so the animation restarts. */
export interface StepSweep {
  key: number;
  label: string;
}

/** The step's name for the sweep: Rust's kind label (`INTERVAL 3/5`), or `WALK 2/3` for a step with no kind. */
export function stepSweepLabel(a: ActiveProgram): string {
  if (a.kind) return a.kind.label;
  return a.step_count > 1 ? `WALK ${a.step_index + 1}/${a.step_count}` : "WALK";
}

/** The step last taken: none before the first snapshot, `program` null with no program running. */
interface Seen {
  known: boolean;
  program: number | null;
  step: string | null;
}

/**
 * Counts step changes while a program runs: a new step (or the next cycle's
 * first) once it walks, not paused or waiting for GO. The step a program
 * starts on is not a change, nor is the step the HUD finds when it opens; a
 * rest or a wait holds the sweep until the step walks. Re-sent snapshots of
 * the same step never sweep again.
 */
export function useStepSweep(a: ActiveProgram | null | undefined): StepSweep | null {
  const [seen, setSeen] = useState<Seen>({ known: false, program: null, step: null });
  const [sweep, setSweep] = useState<StepSweep | null>(null);
  if (a === undefined) return sweep;
  const program = a ? a.program_id : null;
  const step = a ? `${a.cycle}:${a.step_index}` : null;
  const sameRun = seen.known && program !== null && seen.program === program;
  if (!sameRun) {
    // First snapshot, no program, or a program just started: take the step as it is.
    if (!seen.known || seen.program !== program || seen.step !== step) setSeen({ known: true, program, step });
  } else if (a && step !== seen.step && a.phase === "walking" && !a.paused && a.wait === null) {
    setSeen({ known: true, program, step });
    if (!prefersReducedMotion()) setSweep({ key: (sweep?.key ?? 0) + 1, label: stepSweepLabel(a) });
  }
  return sweep;
}
