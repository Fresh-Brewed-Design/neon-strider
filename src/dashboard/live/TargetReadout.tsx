import type { PulseTarget } from "../../ipc";
import { targetAccent, targetRange } from "../../widgets/zone";
import { cx, TONE_TEXT } from "../util";

/**
 * PULSE_RATE's cool-down target on the Live page, beside the stat's name:
 * Rust's word for where the pulse is, then the target. Rust's sentence is the
 * tooltip, and screen readers read it.
 */
export function TargetReadout({ target }: { target: PulseTarget }) {
  const accent = targetAccent(target);
  const tone = accent ? TONE_TEXT[accent] : "text-white/50";
  return (
    <span
      className="ml-auto min-w-0 text-right font-mono text-[11px] leading-snug tracking-[0.08em] [overflow-wrap:anywhere]"
      title={target.message}
      data-testid="live-pulse-target"
    >
      <span className={cx("font-bold", tone)}>{target.label}</span>
      <span className="ml-2 text-white/60">
        {targetRange(target)} <span className="text-white/40">BPM</span>
      </span>
      <span className="sr-only">{target.message}</span>
    </span>
  );
}
