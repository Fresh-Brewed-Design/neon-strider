let context: AudioContext | null = null;

/** The pitches every HUD cue is built from (Hz): low, middle and high. */
export const LOW = 440;
export const MID = 660;
export const HIGH = 880;

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
    // No audio device: the HUD text still says what is happening.
  }
}
