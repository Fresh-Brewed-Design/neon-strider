import { useEffect, useRef } from "react";
import type { EyeBreakSnapshot } from "../ipc";

let context: AudioContext | null = null;

/** One short sine beep per entry of `pitches` (Hz), 180 ms apart. Silent if audio is unavailable. */
export function playTones(pitches: number[]): void {
  try {
    context ??= new AudioContext();
    const ctx = context;
    void ctx.resume();
    pitches.forEach((hz, i) => {
      const start = ctx.currentTime + i * 0.18;
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.frequency.value = hz;
      gain.gain.setValueAtTime(0.0001, start);
      gain.gain.exponentialRampToValueAtTime(0.18, start + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, start + 0.15);
      osc.connect(gain).connect(ctx.destination);
      osc.start(start);
      osc.stop(start + 0.16);
    });
  } catch {
    // No audio device: the card still says what to do.
  }
}

/** A single soft beep: blink now. */
export const BLINK_TONE = [660];
/** Two rising beeps: the break is over. */
export const END_TONE = [660, 880];

/**
 * Plays the blink and end tones when Rust's counters move. The first snapshot only
 * sets the baseline, so opening the HUD never beeps for an old break.
 */
export function useEyeBreakTones(eye: EyeBreakSnapshot | undefined): void {
  const last = useRef<{ blink: number; end: number } | null>(null);
  const blink = eye?.blink_seq;
  const end = eye?.end_seq;
  const sound = eye?.sound ?? false;
  useEffect(() => {
    if (blink === undefined || end === undefined) return;
    const prev = last.current;
    last.current = { blink, end };
    if (!prev || !sound) return;
    if (end !== prev.end) playTones(END_TONE);
    else if (blink !== prev.blink) playTones(BLINK_TONE);
  }, [blink, end, sound]);
}
