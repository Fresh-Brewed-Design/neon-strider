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

/**
 * Kilometres one pass of the program walks (one cycle, when it repeats), from the saved
 * speeds, which are already calibrated. Null with no steps or any step with no length.
 */
export function passDistanceKm(p: Program): number | null {
  if (p.steps.length === 0 || p.steps.some((s) => s.duration_minutes == null)) return null;
  return p.steps.reduce((sum, s) => sum + (s.speed_kmh * (s.duration_minutes ?? 0)) / 60, 0);
}

/** The distance as one decimal with a no-break space before km, or null when it rounds to 0.0. */
function distanceLabel(p: Program): string | null {
  const km = passDistanceKm(p);
  if (km == null) return null;
  const text = km.toFixed(1);
  return text === "0.0" ? null : `≈${text}${NBSP}km`;
}

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
  const pulse = p.steps.some((s) => s.pulse_end) ? " · pulse end" : "";
  const km = distanceLabel(p);
  const distance = km ? ` · ${km}${p.repeat_every_minutes != null ? " a cycle" : ""}` : "";
  return `${length}${repeat} · ${pace}${distance}${zone}${pulse}`;
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
    const pulse = st.pulse_end ? `, then on until heart rate is down to ${num(st.pulse_end.below_pct)} % of max` : "";
    const kind = st.kind ? `${kindCode(st.kind)} ` : "";
    lines.push(`${prefix}${kind}${time} at ${st.speed_kmh.toFixed(1)} km/h, ${num(st.incline_pct)}% incline${zone}${pulse}`);
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
  const km = distanceLabel(p);
  if (km) {
    lines.push(`About ${km.slice(1)} ${p.repeat_every_minutes != null ? "each cycle" : "a pass"} at the set speeds.`);
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
      return "Rest over. GO starts the belt.";
    case "hold_complete":
      return "Hold over. GO starts the belt.";
    case "belt_still":
      return "Belt found still. GO carries on.";
  }
}

/**
 * Status line for a running program, e.g. "Walk 2/3 · 04:12 left" or "Rest · next walk in 21:30".
 * A step with a kind leads with Rust's label, counted within the kind, and keeps the step
 * number after it: "INTERVAL 3/5 · step 6/11 · 02:08 left". A cool-down past its minutes,
 * waiting on the pulse, says so in place of the time: "COOL_DOWN · step 5/5 · pulse to 114".
 */
export function describeActive(a: ActiveProgram): string {
  const left =
    a.pulse_to_bpm != null
      ? `pulse${NBSP}to${NBSP}${a.pulse_to_bpm}`
      : a.seconds_remaining != null
        ? `${formatClock(a.seconds_remaining)} left`
        : null;
  // The plain clock, for a rest and a wait for GO.
  const clock = a.seconds_remaining != null ? formatClock(a.seconds_remaining) : null;
  if (a.phase === "resting") return clock ? `Rest · next walk in ${clock}` : "Rest";
  const position = a.step_count > 1 ? `${a.step_index + 1}/${a.step_count}` : "";
  const cycle = a.cycle > 1 ? ` · cycle ${a.cycle}` : "";
  if (a.kind) {
    // No-break spaces keep "INTERVAL 3/5" and "step 6/11" whole when a narrow HUD wraps the line.
    const step = `${a.kind.label.replace(/ /g, NBSP)}${position ? ` · step${NBSP}${position}` : ""}`;
    if (a.wait) return `Next: ${step}${clock ? ` · ${clock}` : ""}${cycle}`;
    return `${step}${left ? ` · ${left}` : ""}${cycle}`;
  }
  const step = position ? ` ${position}` : "";
  if (a.wait) return `Next: walk${step}${clock ? ` · ${clock}` : ""}${cycle}`;
  return `Walk${step}${left ? ` · ${left}` : ""}${cycle}`;
}

/** The PC clock `seconds` after `now`, in the user's time format with no seconds: "15:40" or "3:40 PM". */
export function finishClock(seconds: number, now: Date, locale?: string): string {
  const end = new Date(now.getTime() + seconds * 1000);
  return new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }).format(end);
}

/**
 * T-MINUS, the whole program's time left and when it ends: "T-MINUS 23:10 · ENDS 15:40".
 * Paused, the end drifts, so only the time left shows. Null when Rust gives no end (a
 * repeating program, or a step to come with no fixed length).
 */
export function describeTimeLeft(a: ActiveProgram, now: Date = new Date(), locale?: string): string | null {
  const left = a.program_seconds_remaining;
  if (left == null) return null;
  // No-break spaces keep the dot on the time left and the end time whole, so a narrow HUD
  // wraps after the dot, or after T-MINUS when even the time left is too wide.
  const tMinus = `T-MINUS ${formatClock(left)}`;
  if (a.paused) return tMinus;
  const ends = finishClock(left, now, locale).replace(/ /g, NBSP);
  return `${tMinus}${NBSP}· ENDS${NBSP}${ends}`;
}
