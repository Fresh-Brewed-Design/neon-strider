import { useEffect, useRef } from "react";
import type { ProgramCue, ProgramTonesSnapshot } from "../ipc";
import { HIGH, LOW, MID, playTones } from "./tones";

/**
 * Program cues, each unlike every eye-break tone so the two never mix up.
 * One high beep: the program moved to its next step.
 */
export const STEP_CUE = [HIGH];
/** High and low twice over: the program waits for GO. */
export const WAIT_CUE = [HIGH, LOW, HIGH, LOW];
/** Three rising beeps: the next step speeds the belt up in a few seconds. */
export const SPEED_UP_CUE = [LOW, MID, HIGH];

const CUE: Record<ProgramCue, number[]> = {
  step: STEP_CUE,
  wait: WAIT_CUE,
  speed_up: SPEED_UP_CUE,
};

/**
 * A cue older than this when it reaches the HUD is skipped. Rust's frames
 * arrive within a quarter second, so 5 s only drops a cue the HUD gets late:
 * a window that showed again or reloaded after it fired.
 */
export const CUE_FRESH_MS = 5000;

/**
 * Plays Rust's program cue when its counter moves. The first snapshot only sets
 * the baseline, so opening or reloading the HUD never beeps for an old step.
 * Sound only: nothing here sends a command.
 */
export function useProgramTones(tones: ProgramTonesSnapshot | undefined, now: () => number = Date.now): void {
  const last = useRef<number | null>(null);
  const seq = tones?.seq;
  const cue = tones?.cue ?? null;
  const at = tones?.at_ms ?? 0;
  const enabled = tones?.enabled ?? false;
  useEffect(() => {
    if (seq === undefined) return;
    const prev = last.current;
    last.current = seq;
    if (prev === null || prev === seq || !enabled || !cue) return;
    if (now() - at > CUE_FRESH_MS) return;
    playTones(CUE[cue]);
  }, [seq, cue, at, enabled, now]);
}
