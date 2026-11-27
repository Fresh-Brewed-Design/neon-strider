import { useEffect, useRef } from "react";
import type { BlinkStep, EyeBreakSnapshot } from "../ipc";
import { HIGH, LOW, MID, playTones } from "./tones";

/**
 * Guided full blink cues, one per step (each step lasts 2 s, set in Rust).
 * Tones, not speech: 45 cues in a 90 s break would make a voice nag, a short
 * sine beep stays calm, and it needs no voice or language on the PC.
 * Falling pair: close your eyes and hold.
 */
export const CLOSE_TONE = [MID, LOW];
/** One low beep: squeeze the eyelids gently. */
export const SQUEEZE_TONE = [LOW];
/** One high beep: open your eyes (the old blink beep). */
export const OPEN_TONE = [MID];
/** Two rising beeps: the break is over. */
export const END_TONE = [MID, HIGH];

const STEP_TONE: Record<BlinkStep, number[]> = {
  close: CLOSE_TONE,
  squeeze: SQUEEZE_TONE,
  open: OPEN_TONE,
};

/**
 * Plays the blink cue and end tones when Rust's counters move, unless Rust says the
 * microphone is in use (`silent`). The first snapshot only sets the baseline, so
 * opening the HUD never beeps for an old break.
 */
export function useEyeBreakTones(eye: EyeBreakSnapshot | undefined): void {
  const last = useRef<{ blink: number; end: number } | null>(null);
  const blink = eye?.blink_seq;
  const end = eye?.end_seq;
  const step = eye?.blink ?? null;
  // No tones while the microphone is in use (a call); the counters still move.
  const sound = (eye?.sound ?? false) && !(eye?.silent ?? false);
  useEffect(() => {
    if (blink === undefined || end === undefined) return;
    const prev = last.current;
    last.current = { blink, end };
    if (!prev || !sound) return;
    if (end !== prev.end) playTones(END_TONE);
    else if (blink !== prev.blink) playTones(step ? STEP_TONE[step] : OPEN_TONE);
  }, [blink, end, step, sound]);
}
