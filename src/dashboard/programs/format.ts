import type { ActiveProgram, Program, ProgramWait, StepKind } from "../../ipc";
import { formatClock } from "../../format";

const num = (v: number) => (Number.isInteger(v) ? v.toString() : v.toFixed(1));

/** Below this a speed holds the belt still: Rust's `engine::STILL_KMH`, half the deck's 0.1 km/h step. */
const MOVING_KMH = 0.05;

/** Each step kind's code, as Rust's `StepKind::code` writes it on the HUD, in the editor's order. */
export const STEP_KINDS: readonly { value: StepKind; code: string }[] = [
  { value: "warm_up", code: "WARM_UP" },
  { value: "walk", code: "WALK" },
  { value: "interval", code: "INTERVAL" },
  { value: "rest", code: "REST" },
  { value: "cool_down", code: "COOL_DOWN" },
];

const NBSP = "\u00a0";

const kindCode = (k: StepKind) => STEP_KINDS.find((x) => x.value === k)?.code ?? k;

/** One-line description, e.g. "5 min every 30 min · 3.0 km/h · 5%". */
export function describeProgram(p: Program): string {
  if (p.steps.length === 0) return "No steps";
  const total = p.steps.reduce((sum, s) => sum + (s.duration_minutes ?? 0), 0);
  const openEnded = p.steps.some((s) => s.duration_minutes == null);
  const first = p.steps[0];
  const pace =
    p.steps.length === 1
      ? `${first.speed_kmh.toFixed(1)} km/h · ${num(first.incline_pct)}%`
      : `${p.steps.length} steps`;

  const length = openEnded ? (p.steps.length === 1 ? "Continuous" : `${num(total)} min, then continuous`) : `${num(total)} min`;
  const repeat = p.repeat_every_minutes != null ? ` every ${num(p.repeat_every_minutes)} min` : "";
  const zone = p.steps.some((s) => s.zone) ? " · heart zone" : "";
  return `${length}${repeat} · ${pace}${zone}`;
}

/** A walking step follows one that holds the belt still (0 km/h). */
function walksAfterAHold(p: Program): boolean {
  return p.steps.some((st, i) => i > 0 && st.speed_kmh >= MOVING_KMH && p.steps[i - 1].speed_kmh < MOVING_KMH);
}

/**
 * The whole plan as hover text: each step, the repeat, and what happens at
 * the end. Plain text with line breaks (used in `title` tooltips).
 */
export function describePlan(p: Program): string {
  const lines = [p.name];
  p.steps.forEach((st, i) => {
    const time = st.duration_minutes == null ? "until you end it" : `${num(st.duration_minutes)} min`;
    const prefix = p.steps.length > 1 ? `${i + 1}. ` : "";
    const zone = st.zone ? `, heart zone ${num(st.zone.low_pct)} to ${num(st.zone.high_pct)} % of max` : "";
    const kind = st.kind ? `${kindCode(st.kind)} ` : "";
    lines.push(`${prefix}${kind}${time} at ${st.speed_kmh.toFixed(1)} km/h, ${num(st.incline_pct)}% incline${zone}`);
  });
  if (walksAfterAHold(p)) {
    lines.push("After a 0 km/h step, the belt waits for GO before it moves.");
  }
  if (p.repeat_every_minutes != null) {
    lines.push(`Repeats every ${num(p.repeat_every_minutes)} min; back to your previous speed in between.`);
    lines.push("After each rest, the belt waits for GO before it moves.");
  } else if (p.steps.every((st) => st.duration_minutes != null)) {
    lines.push("Then back to what the treadmill was doing before.");
  }
  return lines.join("\n");
}

/** The HUD headline while a program waits for GO, e.g. "REST COMPLETE // PRESS GO". */
export function waitHeadline(w: ProgramWait): string {
  switch (w) {
    case "rest_complete":
      return "REST COMPLETE // PRESS GO";
    case "hold_complete":
      return "HOLD COMPLETE // PRESS GO";
    case "belt_still":
      return "BELT STILL // PRESS GO";
  }
}

/** Why the program is waiting, in plain words. */
export function waitReason(w: ProgramWait): string {
  switch (w) {
    case "rest_complete":
      return "The rest is over. The belt starts when you press GO.";
    case "hold_complete":
      return "The next step walks. The belt starts when you press GO.";
    case "belt_still":
      return "The belt was found still after a reconnect. Press GO to carry on.";
  }
}

/**
 * Status line for a running program, e.g. "Walk 2/3 · 04:12 left" or "Rest · next walk in 21:30".
 * A step with a kind leads with Rust's label, counted within the kind, and keeps the step
 * number after it: "INTERVAL 3/5 · step 6/11 · 02:08 left".
 */
export function describeActive(a: ActiveProgram): string {
  const left = a.seconds_remaining != null ? formatClock(a.seconds_remaining) : null;
  if (a.phase === "resting") return left ? `Rest · next walk in ${left}` : "Rest";
  const position = a.step_count > 1 ? `${a.step_index + 1}/${a.step_count}` : "";
  const cycle = a.cycle > 1 ? ` · cycle ${a.cycle}` : "";
  if (a.kind) {
    // No-break spaces keep "INTERVAL 3/5" and "step 6/11" whole when a narrow HUD wraps the line.
    const step = `${a.kind.label.replace(/ /g, NBSP)}${position ? ` · step${NBSP}${position}` : ""}`;
    if (a.wait) return `Next: ${step}${left ? ` · ${left}` : ""}${cycle}`;
    return `${step}${left ? ` · ${left} left` : ""}${cycle}`;
  }
  const step = position ? ` ${position}` : "";
  if (a.wait) return `Next: walk${step}${left ? ` · ${left}` : ""}${cycle}`;
  return `Walk${step}${left ? ` · ${left} left` : ""}${cycle}`;
}
