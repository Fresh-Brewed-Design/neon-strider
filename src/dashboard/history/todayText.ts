import type { TimelineMark, TimelineRun, TodayTimeline } from "../../ipc";

/** Most hour labels under the strip: six fit at 760 px even at the largest Windows text size. */
const MAX_LABELS = 6;
/** Hours between labels, smallest first, so labels land on even clock hours. */
const LABEL_STEPS = [1, 2, 3, 4, 6];

/** `08:05`: minutes after local midnight as a clock time. */
export function clockLabel(minute: number): string {
  const m = Math.max(0, Math.min(24 * 60, Math.round(minute)));
  return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
}

/** The plain sentence beside the label. */
export function todayText(t: TodayTimeline): string {
  return `From ${clockLabel(t.start_min)} to ${clockLabel(t.now_min)}, in ${t.slot_min}-minute slots.`;
}

export const MARK_WORDS: Record<TimelineMark, string> = {
  walked: "Walking",
  desk: "At the PC",
  away: "Away",
};

const RUN_WORDS: Record<TimelineMark, string> = {
  walked: "walking",
  desk: "at the PC",
  away: "away",
};

/** A run's tooltip and accessible name. */
export function runTitle(run: TimelineRun): string {
  return `${clockLabel(run.start_min)} to ${clockLabel(run.end_min)}: ${RUN_WORDS[run.mark]}`;
}

/** Whole hours to label under the strip, at most `MAX_LABELS` of them. */
export function hourTicks(t: TodayTimeline): number[] {
  const first = Math.ceil(t.start_min / 60);
  const last = Math.floor(t.now_min / 60);
  if (last < first) return [];
  const span = last - first + 1;
  const step = LABEL_STEPS.find((s) => Math.ceil(span / s) <= MAX_LABELS) ?? 24;
  const ticks: number[] = [];
  // Even steps land on clock hours divisible by the step (06, 09, 12 for 3).
  for (let h = Math.ceil(first / step) * step; h <= last; h += step) ticks.push(h);
  return ticks;
}
