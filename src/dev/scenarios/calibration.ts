// GROUND_TRUTH, the guided calibration. ?calwizard: a 280 cm belt loop, two
// saved points and the top speed question. ?calwizard=belt: no belt length
// yet (the BELT_LOOP step). ?calwizard=tapping: a run at 3.0 km/h five passes
// in (the clock ticks); =dropped the same with one tap dropped and a restart
// note; =measured: the taps agreed and the result waits for SAVE POINT.
// ?calwizard=sweep: RUN ALL with two speeds done and 4.0 km/h up next;
// ?calwizard=sweeprun: the same sweep tapping at 4.0. ?calwizard=steps: the
// measured run tapping footsteps (GAIT_SYNC) twelve steps in; =stepsaved the
// stride measured and saved, with UNDO. ?calwizard=guiderun: the guided walk
// from 0.6 to 5.0 km/h tapping at 5.0; =guidecheck both ends saved and the
// midpoint check up next; =guidesplit the check 3.1 % off and split;
// =guidedone the walk done with its summary; =guidemoved RUN asked 0.6, the
// deck ran 1.2 (a start at the treadmill), and the walk's slowest moved there;
// =guideskip both ends saved on a deck that skips 2.8, so the check moved to 2.7;
// =guideran the 2.8 check the deck ran at 2.9, split there.
// ?calwizard=aside: START FRESH set the two points aside and the walk starts
// from none (POINTS_ASIDE with RESTORE and KEEP NEW); =asidedone the new walk
// done with the old set still aside.
// ?caloffer: a connected treadmill
// with no calibration, so the first-connect card shows. The math is a rough
// stand-in for Rust's (calibration.rs and calibration/tap.rs, unit-tested there).
import type { CalibrationGuideView, CalibrationRunView, Command, GuideCheck, GuideSpeed, StepRunView } from "../../ipc";
import { PREVIEW_CONFIG, params, publish, snapshot } from "../mockState";
import { useTreadmillStore } from "../../store/treadmillStore";
import type { Scenario } from "./types";

const calFlag = params.get("calwizard");
const offerFlag = params.has("caloffer");
const BELT_CM = 280;
let calTimer: ReturnType<typeof setInterval> | null = null;
/** The mock's tap times, ms, since the tapping last started. */
let taps: number[] = [];
/** The mock's footstep times, ms. */
let stepTaps: number[] = [];

function median(v: number[]): number | null {
  if (v.length === 0) return null;
  const s = [...v].sort((a, b) => a - b);
  const n = s.length;
  return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
}

/** Rust's tap maths in short: gaps within 15 % of the median count; eight settle the run. */
function tapStats(times: number[]) {
  const gaps = times.slice(1).map((t, i) => (t - times[i]) / 1000);
  const m = median(gaps);
  const kept = m === null ? [] : gaps.filter((g) => Math.abs(g - m) / m <= 0.15);
  return { kept, dropped: gaps.length - kept.length, median: median(kept) };
}

function calRun(target_kmh: number, view: Partial<CalibrationRunView> = {}) {
  if (calTimer) clearInterval(calTimer);
  calTimer = null;
  snapshot.calibration.run = {
    target_kmh,
    phase: "settling",
    passes: 0,
    kept: 0,
    dropped: 0,
    elapsed_s: 0,
    measured_kmh: null,
    reported_kmh: null,
    note: null,
    result: null,
    saved: false,
    undone: false,
    steps: null,
    ...view,
  };
  if (snapshot.calibration.run.phase === "tapping") {
    calTimer = setInterval(() => {
      const r = snapshot.calibration.run;
      if (r?.phase === "tapping") r.elapsed_s += 0.25;
      publish();
    }, 250);
  }
}
function closeCal() {
  if (calTimer) clearInterval(calTimer);
  calTimer = null;
  taps = [];
  snapshot.calibration.run = null;
}
/** A measured run's result: the deck at `reported`, the belt really at `real`. */
function measured(target_kmh: number, real: number, reported: number, passes: number, atLoad = false) {
  const elapsed_s = ((passes - 1) * BELT_CM) / 100 / (real / 3.6);
  const saved = CAL_CONFIG.calibration_points.find((p) => Math.abs(p.speed - target_kmh) <= 0.05);
  const scale = reported / real;
  const n = Math.max(1, saved?.runs ?? 1);
  const averaged = saved
    ? {
        speed: saved.speed,
        stride_length_cm: saved.stride_length_cm,
        distance_scale: (saved.distance_scale * n + scale) / (n + 1),
        runs: n + 1,
        stride_runs: saved.stride_runs ?? 0,
      }
    : null;
  const point = { speed: Math.round(reported * 10) / 10, stride_length_cm: 72, distance_scale: scale, runs: 1, stride_runs: 0 };
  // Rust saves the point by itself and keeps what it replaced for UNDO. At load the
  // saved list already holds it (CAL_CONFIG), so the preview starts as Rust leaves it.
  before = saved ? { ...saved } : null;
  const written = averaged ?? point;
  const sweep = snapshot.calibration.sweep;
  if (sweep) {
    const i = sweep.targets_kmh.findIndex((t) => Math.abs(t - target_kmh) <= 0.05);
    if (i >= 0) sweep.marks[i] = "saved";
    sweepCurrent();
  }
  guideRecord(target_kmh, point.speed, scale);
  if (!atLoad) {
    setPoints([...CAL_CONFIG.calibration_points.filter((p) => Math.abs(p.speed - written.speed) > 0.05), written]);
  }
  calRun(target_kmh, {
    saved: true,
    phase: "measured",
    passes,
    kept: passes - 1,
    elapsed_s,
    measured_kmh: real,
    reported_kmh: reported,
    result: {
      point,
      real_kmh: real,
      reported_kmh: reported,
      real_m: (real / 3.6) * elapsed_s,
      reported_m: (reported / 3.6) * elapsed_s,
      cadence_spm: null,
      stride_measured: false,
      averaged,
    },
  });
}
/** What the mock's last save replaced, for UNDO. */
let before: CalPoint | null = null;
type CalPoint = { speed: number; stride_length_cm: number; distance_scale: number; runs?: number; stride_runs?: number };
/** Put the points back as before the run's save; its sweep speed is up again. */
function undoSave() {
  const r = snapshot.calibration.run;
  if (!r?.saved || !r.result) return;
  // A stride saved on top goes first, as in Rust.
  undoSteps();
  r.steps = null;
  const written = r.result.averaged ?? r.result.point;
  const kept = CAL_CONFIG.calibration_points.filter((p) => Math.abs(p.speed - written.speed) > 0.05);
  setPoints(before ? [...kept, before] : kept);
  r.saved = false;
  const s = snapshot.calibration.sweep;
  if (s) {
    const i = s.targets_kmh.findIndex((t) => Math.abs(t - r.target_kmh) <= 0.05);
    if (i >= 0 && s.marks[i] === "saved") s.marks[i] = "pending";
    sweepCurrent();
  }
  if (r.result) guideOutcomes.delete(key(r.result.point.speed));
  guideForget(r.target_kmh);
}
/** The saved points changed in Rust: the dashboard reloads them when `points_rev` moves. */
function setPoints(points: CalPoint[]) {
  CAL_CONFIG.calibration_points = points.sort((a, b) => a.speed - b.speed);
  snapshot.calibration.points_rev += 1;
}
function stepView(view: Partial<StepRunView> = {}): StepRunView {
  return {
    phase: "settling",
    steps: 0,
    kept: 0,
    dropped: 0,
    elapsed_s: 0,
    cadence_spm: null,
    stride_cm: null,
    note: null,
    result: null,
    saved: false,
    undone: false,
    ...view,
  };
}
/** What the mock's stride save replaced, for its UNDO. */
let stepBefore: CalPoint | null = null;
/**
 * Steps measured on the open run, `gap` s apart, and saved by themselves: the
 * stride joins the point at the speed, its factor and run count kept.
 */
function stepsMeasured(gap: number, count: number, atLoad = false) {
  const r = snapshot.calibration.run;
  if (!r?.result) return;
  const stride = (r.result.real_kmh / 3.6) * gap * 100;
  const saved = CAL_CONFIG.calibration_points.find((p) => Math.abs(p.speed - r.target_kmh) <= 0.05);
  const n = Math.max(1, saved?.runs ?? 1);
  const m = saved?.stride_runs ?? 0;
  const point = { ...r.result.point, stride_length_cm: stride, runs: 1, stride_runs: 1 };
  // As in Rust: a stride never measured is replaced; later ones average with measured strides only.
  const averaged = saved
    ? { ...saved, runs: n, stride_runs: m + 1, stride_length_cm: (saved.stride_length_cm * m + stride) / (m + 1) }
    : null;
  const written = averaged ?? point;
  stepBefore = saved ? { ...saved } : null;
  if (!atLoad) {
    setPoints([...CAL_CONFIG.calibration_points.filter((p) => Math.abs(p.speed - written.speed) > 0.05), written]);
  }
  r.steps = stepView({
    phase: "measured",
    steps: count,
    kept: count - 1,
    elapsed_s: (count - 1) * gap,
    cadence_spm: 60 / gap,
    stride_cm: stride,
    result: { ...r.result, point, cadence_spm: 60 / gap, stride_measured: true, averaged },
    saved: true,
  });
}
/** Put the point back as the belt run left it. */
function undoSteps() {
  const st = snapshot.calibration.run?.steps;
  if (!st?.saved || !st.result) return false;
  const written = st.result.averaged ?? st.result.point;
  const kept = CAL_CONFIG.calibration_points.filter((p) => Math.abs(p.speed - written.speed) > 0.05);
  setPoints(stepBefore ? [...kept, stepBefore] : kept);
  st.saved = false;
  return true;
}
function sweepCurrent() {
  const s = snapshot.calibration.sweep;
  if (!s) return null;
  const i = s.marks.indexOf("pending");
  s.current = i < 0 ? null : i;
  return s.current === null ? null : s.targets_kmh[s.current];
}
/** The mock's guided walk: the ends, then midpoint checks split past 2 %, three levels down (Rust: calibration/guide.rs). */
let guideEnds: [number, number] | null = null;
/** The walk's note after a run the deck ran at another speed (Rust: `Guide::record`). */
let guideNote: string | null = null;
const guideOutcomes = new Map<number, { at: number; scale: number } | "skipped">();
const key = (kmh: number) => Math.round(kmh * 10);
/** Speeds DECK_PROFILE knows the deck skips, as tenths (Rust: `Guide::with_skipped`). */
let guideSkips = new Set<number>();
/** The check's speed between `lo` and `hi`: halfway, else the nearest the deck runs, slower first (Rust: `check_speed`). */
function guideCheckSpeed(lo: number, hi: number): { mid: number; halfway: number | null } | null {
  const half = Math.round((lo + hi) * 5);
  const inside = (t: number) => t > key(lo) && t < key(hi);
  if (!inside(half)) return null;
  if (!guideSkips.has(half)) return { mid: half / 10, halfway: null };
  for (let d = 1; inside(half - d) || inside(half + d); d++) {
    for (const t of [half - d, half + d]) if (inside(t) && !guideSkips.has(t)) return { mid: t / 10, halfway: half / 10 };
  }
  return null;
}
function guideRecord(target: number, at: number, scale: number) {
  guideNote = null;
  if (guideEnds && key(at) !== key(target) && key(target) === key(guideEnds[0]) && at < guideEnds[1]) {
    guideNote = `The deck ran ${at.toFixed(1)} km/h when asked for ${target.toFixed(1)}, so ${at.toFixed(1)} km/h is the slowest speed.`;
    guideOutcomes.delete(key(target));
    guideEnds = [at, guideEnds[1]];
    target = at;
  }
  if (guideEnds) guideOutcomes.set(key(target), { at, scale });
  guidePublish();
}
function guideForget(target: number) {
  guideOutcomes.delete(key(target));
  guidePublish();
}
function guidePlan(): CalibrationGuideView | null {
  if (!guideEnds) return null;
  const [slow, top] = guideEnds;
  const speeds: GuideSpeed[] = [];
  const checks: GuideCheck[] = [];
  let next: number | null = null;
  /** The speed the deck ran, when it measured another than planned (Rust: `Guide::ran`). */
  const ran = (kmh: number) => {
    const o = guideOutcomes.get(key(kmh));
    return o === undefined || o === "skipped" || key(o.at) === key(kmh) ? null : key(o.at) / 10;
  };
  const mark = (kmh: number) => {
    const o = guideOutcomes.get(key(kmh));
    return o === undefined ? "pending" : o === "skipped" ? "skipped" : "saved";
  };
  // A split's halves can meet where the deck ran, so a span end is also found by the speed run there.
  const measuredAt = (kmh: number) => {
    const o = guideOutcomes.get(key(kmh)) ?? [...guideOutcomes.values()].find((v) => v !== "skipped" && key(v.at) === key(kmh));
    return o === undefined || o === "skipped" ? null : o;
  };
  for (const end of [slow, top]) {
    speeds.push({ kmh: end, ran_kmh: ran(end), mark: mark(end), end: true });
    if (!guideOutcomes.has(key(end)) && next === null) next = end;
  }
  const walk = (lo: number, hi: number, depth: number) => {
    const at = guideCheckSpeed(lo, hi);
    if (!at) return;
    const mid = at.mid;
    const ranAt = ran(mid);
    speeds.push({ kmh: mid, ran_kmh: ranAt, mark: mark(mid), end: false });
    const o = guideOutcomes.get(key(mid));
    const check: GuideCheck = {
      lo_kmh: lo,
      hi_kmh: hi,
      mid_kmh: mid,
      halfway_kmh: at.halfway,
      ran_kmh: ranAt,
      verdict: "pending",
      error_pct: null,
    };
    checks.push(check);
    if (o === undefined) {
      if (next === null) next = mid;
      return;
    }
    if (o === "skipped") {
      check.verdict = "skipped";
      return;
    }
    const a = measuredAt(lo);
    const b = measuredAt(hi);
    if (!a || !b) return;
    const t = (o.at - a.at) / (b.at - a.at);
    const predicted = a.scale + t * (b.scale - a.scale);
    const err = Math.abs(o.scale / predicted - 1);
    check.error_pct = err * 100;
    check.verdict = err <= 0.02 ? "agreed" : depth >= 3 ? "at_limit" : "split";
    if (check.verdict === "split") {
      // The halves meet where the deck ran, when that is inside the span.
      const m = ranAt !== null && ranAt > lo && ranAt < hi ? ranAt : mid;
      walk(lo, m, depth + 1);
      walk(m, hi, depth + 1);
    }
  };
  if (measuredAt(slow) && measuredAt(top)) walk(slow, top, 1);
  speeds.sort((x, y) => (x.ran_kmh ?? x.kmh) - (y.ran_kmh ?? y.kmh));
  const ended = checks.filter((c) => c.verdict === "agreed" || c.verdict === "at_limit").map((c) => c.error_pct ?? 0);
  return {
    slow_kmh: slow,
    top_kmh: top,
    speeds,
    checks,
    next_kmh: next,
    remaining_error_pct: ended.length ? Math.max(...ended) : null,
    tolerance_pct: 2,
    note: guideNote,
  };
}
function guidePublish() {
  snapshot.calibration.guide = guidePlan();
}
/** Load a walk from 0.6 to 5.0 with these speeds already measured, as factors. */
function guideLoad(measured: [number, number][], skips: number[] = []) {
  guideEnds = [0.6, 5];
  guideSkips = new Set(skips.map(key));
  guideOutcomes.clear();
  for (const [kmh, scale] of measured) guideOutcomes.set(key(kmh), { at: kmh, scale });
  guidePublish();
}
function runBelt(kmh: number) {
  Object.assign(snapshot.deck, { status: "Running", raw_speed_kmh: kmh, speed_kmh: kmh });
  taps = [];
  calRun(kmh);
}
/** ?calwizard: two saved points; the old app-wide belt length, 300 cm, prefills ?calwizard=belt. */
// ?calwizard=oldpoint adds a point stored at 0.15, below the limit.
// ?calwizard=measured starts with the 3.0 km/h run already averaged in (3 runs).
const SAVED_POINTS: CalPoint[] = [
  calFlag === "measured"
    ? { speed: 3, stride_length_cm: 68.4, distance_scale: (0.912 * 2 + 3 / 2.82) / 3, runs: 3, stride_runs: 1 }
    : { speed: 3, stride_length_cm: 68.4, distance_scale: 0.912, runs: 2, stride_runs: 1 },
  { speed: 4.5, stride_length_cm: 74.1, distance_scale: calFlag === "oldpoint" ? 0.15 : 0.887 },
];
/** ?calwizard=asidedone: the points the new walk saved (=guidedone's speeds). */
const NEW_WALK_POINTS: CalPoint[] = (
  [
    [0.6, 0.86],
    [1.7, 0.925],
    [2.8, 0.98],
    [3.9, 1.015],
    [5, 1.04],
  ] as const
).map(([speed, distance_scale]) => ({ speed, stride_length_cm: 66, distance_scale, runs: 1, stride_runs: 0 }));
/** START FRESH's set aside (Rust keeps it per treadmill); null when none is aside. */
let aside: CalPoint[] | null = calFlag === "aside" || calFlag === "asidedone" ? SAVED_POINTS : null;
const CAL_CONFIG: { belt_length: number; calibration_points: CalPoint[] } = {
  belt_length: 300,
  calibration_points: calFlag === "aside" ? [] : calFlag === "asidedone" ? NEW_WALK_POINTS : SAVED_POINTS,
};
/** The preview runs no settings sync, so a points change from Rust is copied into the store here. */
function pointsToStore() {
  useTreadmillStore.getState().updateSettings({ calibrationPoints: CAL_CONFIG.calibration_points });
}
function publishAside() {
  snapshot.calibration.aside_count = aside === null ? null : aside.length;
}
/** The preview runs no settings sync: the store settings ?calwizard starts with. */
export function previewSettings() {
  return calFlag === null ? null : { beltLength: CAL_CONFIG.belt_length, calibrationPoints: CAL_CONFIG.calibration_points };
}
const CAL_COMMANDS: Partial<Record<Command["type"], (c: Command) => void>> = {
  calibration_run_start: (c) => {
    if (c.type !== "calibration_run_start") return;
    snapshot.calibration.sweep = null;
    guideEnds = null;
    guidePublish();
    runBelt(c.target_kmh);
  },
  calibration_guide_start: (c) => {
    if (c.type !== "calibration_guide_start") return;
    if (!(c.top_kmh > 0.6)) throw "Pick a top speed above the slowest, 0.6 km/h.";
    if (c.top_kmh > 12) throw "The treadmill tops out at 12.0 km/h.";
    guideEnds = [0.6, Math.round(c.top_kmh * 10) / 10];
    guideOutcomes.clear();
    guideSkips = new Set();
    snapshot.calibration.sweep = null;
    closeCal();
    guidePublish();
  },
  calibration_guide_run: () => {
    const next = snapshot.calibration.guide?.next_kmh ?? null;
    if (next === null) throw "Every speed is done.";
    runBelt(next);
  },
  calibration_guide_skip: () => {
    const next = snapshot.calibration.guide?.next_kmh ?? null;
    if (next === null) throw "Every speed is done.";
    guideOutcomes.set(key(next), "skipped");
    closeCal();
    guidePublish();
  },
  calibration_guide_end: () => {
    guideEnds = null;
    closeCal();
    guidePublish();
  },
  calibration_start_fresh: () => {
    if (snapshot.calibration.run || snapshot.calibration.sweep || snapshot.calibration.guide) {
      throw "End the open run or walk first.";
    }
    if (aside !== null) throw "Old points are already set aside.";
    if (CAL_CONFIG.calibration_points.length === 0) throw "There are no points to set aside.";
    aside = CAL_CONFIG.calibration_points;
    setPoints([]);
    pointsToStore();
    publishAside();
  },
  calibration_restore: () => {
    if (aside === null) throw "No old points are set aside.";
    snapshot.calibration.sweep = null;
    guideEnds = null;
    closeCal();
    guidePublish();
    setPoints([...aside]);
    aside = null;
    pointsToStore();
    publishAside();
  },
  calibration_keep_new: () => {
    if (aside === null) throw "No old points are set aside.";
    aside = null;
    publishAside();
  },
  calibration_offer_skip: () => {
    snapshot.calibration.offer = false;
  },
  calibration_target_add: (c) => {
    if (c.type !== "calibration_target_add") return;
    const t = snapshot.calibration.targets;
    if (Math.abs(c.kmh * 10 - Math.round(c.kmh * 10)) > 1e-3) throw "Pick a speed in 0.1 km/h steps.";
    if (c.kmh < 0.6) throw "Pick a speed of at least 0.6 km/h.";
    if (c.kmh > 12) throw "The treadmill tops out at 12.0 km/h.";
    if (t.some((x) => Math.abs(x.kmh - c.kmh) <= 0.05)) throw `${c.kmh.toFixed(1)} km/h is already in the list.`;
    snapshot.calibration.targets = [...t, { kmh: Math.round(c.kmh * 10) / 10, runnable: true }].sort((a, b) => a.kmh - b.kmh);
  },
  calibration_target_remove: (c) => {
    if (c.type !== "calibration_target_remove") return;
    if (snapshot.calibration.run && Math.abs(snapshot.calibration.run.target_kmh - c.kmh) <= 0.05) {
      throw `Close the run at ${c.kmh.toFixed(1)} km/h first.`;
    }
    snapshot.calibration.targets = snapshot.calibration.targets.filter((x) => Math.abs(x.kmh - c.kmh) > 0.05);
  },
  calibration_sweep_start: () => {
    guideEnds = null;
    guidePublish();
    const listed = snapshot.calibration.targets.filter((t) => t.runnable).map((t) => t.kmh);
    const targets_kmh = [...new Set([...listed, ...CAL_CONFIG.calibration_points.map((p) => p.speed)])].sort(
      (a, b) => a - b,
    );
    snapshot.calibration.sweep = { targets_kmh, marks: targets_kmh.map(() => "pending"), current: 0 };
    runBelt(targets_kmh[0]);
  },
  calibration_sweep_run: () => {
    const kmh = sweepCurrent();
    if (kmh === null) throw "Every speed is done.";
    runBelt(kmh);
  },
  calibration_sweep_next: (c) => {
    const s = snapshot.calibration.sweep;
    if (c.type !== "calibration_sweep_next" || !s || s.current === null) return;
    s.marks[s.current] = c.saved ? "saved" : "skipped";
    sweepCurrent();
    closeCal();
  },
  calibration_sweep_end: () => {
    snapshot.calibration.sweep = null;
    closeCal();
  },
  // STOP: the belt stops; a run not yet measured closes.
  stop: () => {
    Object.assign(snapshot.deck, { status: "Stopped", raw_speed_kmh: 0, speed_kmh: 0 });
    if (snapshot.calibration.run?.phase !== "measured") closeCal();
  },
  calibration_belt_set: (c) => {
    if (c.type !== "calibration_belt_set") return;
    if (!(c.cm >= 50 && c.cm <= 1000)) throw "Belt length must be 50 to 1000 cm.";
    snapshot.calibration.belt_cm = c.cm;
  },
  calibration_tap: () => {
    const r = snapshot.calibration.run;
    if (!r) throw "Press RUN on a speed first.";
    if (r.steps) {
      if (r.steps.result) {
        r.steps.note = "The stride is measured. TAP AGAIN starts the steps over.";
        return;
      }
      const now = Date.now();
      if (stepTaps.length && now - stepTaps[stepTaps.length - 1] < 250) return;
      stepTaps.push(now);
      const s = tapStats(stepTaps);
      if (s.kept.length >= 20 && s.median !== null) {
        stepsMeasured(s.median, stepTaps.length);
        return;
      }
      r.steps = stepView({
        phase: "tapping",
        steps: stepTaps.length,
        kept: s.kept.length,
        dropped: s.dropped,
        elapsed_s: (now - stepTaps[0]) / 1000,
        cadence_spm: s.median === null ? null : 60 / s.median,
        stride_cm: s.median === null || !r.result ? null : (r.result.real_kmh / 3.6) * s.median * 100,
      });
      return;
    }
    if (r.phase === "measured") {
      r.note = "The run is measured. TAP AGAIN starts the run over.";
      return;
    }
    const reported = snapshot.deck.raw_speed_kmh;
    if (reported < 0.05) {
      taps = [];
      calRun(r.target_kmh, { note: "The belt is not moving. Press RUN, then tap once it is up to speed." });
      return;
    }
    const now = Date.now();
    if (taps.length && now - taps[taps.length - 1] < 250) return;
    taps.push(now);
    const s = tapStats(taps);
    const real = s.median === null ? null : (BELT_CM / 100 / s.median) * 3.6;
    if (s.kept.length >= 8 && real !== null) {
      measured(r.target_kmh, real, reported, taps.length);
      return;
    }
    calRun(r.target_kmh, {
      phase: "tapping",
      passes: taps.length,
      kept: s.kept.length,
      dropped: s.dropped,
      elapsed_s: (now - taps[0]) / 1000,
      measured_kmh: real,
      reported_kmh: reported,
    });
  },
  calibration_tap_reset: () => {
    const r = snapshot.calibration.run;
    if (!r) throw "Press RUN on a speed first.";
    if (r.saved) undoSave();
    taps = [];
    calRun(r.target_kmh);
  },
  calibration_undo: () => {
    if (!snapshot.calibration.run?.saved) throw "There is no saved point to undo.";
    undoSave();
    snapshot.calibration.run.undone = true;
  },
  calibration_run_close: () => closeCal(),
  calibration_steps_start: () => {
    const r = snapshot.calibration.run;
    if (!r?.result) throw "Time the belt first, then tap each footstep.";
    if (r.undone) throw "UNDO took the point at this speed back. TAP AGAIN times the belt again first.";
    undoSteps();
    stepTaps = [];
    r.steps = stepView();
  },
  calibration_steps_skip: () => {
    const r = snapshot.calibration.run;
    if (r) r.steps = null;
  },
  calibration_steps_undo: () => {
    const s = snapshot.calibration.run?.steps;
    if (!s || !undoSteps()) throw "There is no saved stride to undo.";
    s.undone = true;
  },
};

/** A run `passes` taps in, as if tapped live, with the deck at `reported`. */
function tappingRun(target: number, passes: number, reported: number, extra: Partial<CalibrationRunView> = {}) {
  calRun(target, {
    phase: "tapping",
    passes,
    kept: passes - 1,
    elapsed_s: (passes - 1) * 3.6 + 1.4,
    measured_kmh: 2.81,
    reported_kmh: reported,
    ...extra,
  });
}

export const calibration: Scenario = {
  apply() {
    if (offerFlag) {
      Object.assign(snapshot.calibration, { offer: true, belt_treadmill: true, belt_cm: null });
    }
    if (calFlag === null) return;
    snapshot.calibration.belt_treadmill = true;
    publishAside();
    if (calFlag === "asidedone") guideLoad([[0.6, 0.86], [5, 1.04], [2.8, 0.98], [1.7, 0.925], [3.9, 1.015]]);
    snapshot.calibration.belt_cm = calFlag === "belt" ? null : BELT_CM;
    const live = ["tapping", "dropped", "measured", "steps", "stepsaved"].includes(calFlag);
    if (live) Object.assign(snapshot.deck, { status: "Running", raw_speed_kmh: 3, speed_kmh: 3.3 });
    if (calFlag === "tapping") tappingRun(3, 5, 3);
    if (calFlag === "dropped") {
      tappingRun(3, 7, 3, { kept: 5, dropped: 1, note: "The speed changed (3.0 to 3.5 km/h), so tapping starts again." });
    }
    if (calFlag === "measured") {
      measured(3, 2.82, 3, 9, true);
      before = { speed: 3, stride_length_cm: 68.4, distance_scale: 0.912, runs: 2 };
    }
    if (calFlag === "steps" || calFlag === "stepsaved") {
      measured(3, 2.82, 3, 9, true);
      before = { speed: 3, stride_length_cm: 68.4, distance_scale: 0.912, runs: 2 };
      const run = snapshot.calibration.run;
      if (run && calFlag === "steps") {
        run.steps = stepView({ phase: "tapping", steps: 12, kept: 10, dropped: 1, elapsed_s: 6.9, cadence_spm: 104, stride_cm: 45.2 });
      }
      if (calFlag === "stepsaved") stepsMeasured(0.58, 22, true);
    }
    // The guided walk from 0.6 to 5.0 km/h, on a deck whose factor bends after 2.8 km/h.
    if (calFlag === "guiderun") {
      guideLoad([[0.6, 0.86]]);
      Object.assign(snapshot.deck, { status: "Running", raw_speed_kmh: 5, speed_kmh: 5.2 });
      tappingRun(5, 6, 5, { measured_kmh: 4.82 });
    }
    if (calFlag === "guidemoved") {
      guideLoad([]);
      Object.assign(snapshot.deck, { status: "Running", raw_speed_kmh: 1.2, speed_kmh: 1.2 });
      measured(0.6, 1.12, 1.2, 9);
    }
    if (calFlag === "guidecheck") guideLoad([[0.6, 0.86], [5, 1.04]]);
    if (calFlag === "guideskip") guideLoad([[0.6, 0.86], [5, 1.04]], [2.8]);
    if (calFlag === "guideran") {
      guideLoad([[0.6, 0.86], [5, 1.04]]);
      guideOutcomes.set(key(2.8), { at: 2.9, scale: 0.99 });
      guideNote = "The deck ran 2.9, not 2.8, so the check uses 2.9.";
      guidePublish();
    }
    if (calFlag === "guidesplit") guideLoad([[0.6, 0.86], [5, 1.04], [2.8, 0.98]]);
    if (calFlag === "guidedone") guideLoad([[0.6, 0.86], [5, 1.04], [2.8, 0.98], [1.7, 0.925], [3.9, 1.015]]);
    if (calFlag.startsWith("guide") && calFlag !== "guiderun" && calFlag !== "guidemoved") {
      Object.assign(snapshot.deck, { status: "Running", raw_speed_kmh: 5, speed_kmh: 5.2 });
    }
    // A stored 14.0 km/h the treadmill (12.0 max) can't run any more, and a 3.5 the user added.
    if (calFlag === "targets") {
      snapshot.calibration.targets = [2, 3, 3.5, 4, 5, 6, 14].map((kmh) => ({ kmh, runnable: kmh <= 12 }));
    }
    if (calFlag === "sweep" || calFlag === "sweeprun") {
      Object.assign(snapshot.deck, { status: "Running", raw_speed_kmh: 3, speed_kmh: 3.3 });
      snapshot.calibration.sweep = {
        targets_kmh: [2, 3, 4, 4.5, 5, 6],
        marks: ["skipped", "saved", "pending", "pending", "pending", "pending"],
        current: 2,
      };
      if (calFlag === "sweeprun") {
        Object.assign(snapshot.deck, { raw_speed_kmh: 4, speed_kmh: 4.4 });
        tappingRun(4, 6, 4, { measured_kmh: 3.78 });
      }
    }
  },
  commands: CAL_COMMANDS,
  queries: calFlag !== null ? { get_rust_config: () => ({ ...PREVIEW_CONFIG, ...CAL_CONFIG }) } : {},
};
