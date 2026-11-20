// GROUND_TRUTH, the guided calibration (Labs). ?calwizard: the switch is on,
// a 280 cm belt loop and two saved points. ?calwizard=belt: no belt length
// yet (the BELT_LOOP step). ?calwizard=tapping: a run at 3.0 km/h five passes
// in (the clock ticks); =dropped the same with one tap dropped and a restart
// note; =measured: the taps agreed and the result waits for SAVE POINT.
// ?calwizard=sweep: RUN ALL with two speeds done and 4.0 km/h up next;
// ?calwizard=sweeprun: the same sweep tapping at 4.0. The math is a rough
// stand-in for Rust's (calibration.rs and calibration/tap.rs, unit-tested there).
import type { CalibrationRunView, Command } from "../../ipc";
import { PREVIEW_CONFIG, params, publish, snapshot } from "../mockState";
import type { Scenario } from "./types";

const calFlag = params.get("calwizard");
const BELT_CM = 280;
let calTimer: ReturnType<typeof setInterval> | null = null;
/** The mock's tap times, ms, since the tapping last started. */
let taps: number[] = [];

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
      }
    : null;
  const point = { speed: Math.round(reported * 10) / 10, stride_length_cm: 72, distance_scale: scale, runs: 1 };
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
type CalPoint = { speed: number; stride_length_cm: number; distance_scale: number; runs?: number };
/** Put the points back as before the run's save; its sweep speed is up again. */
function undoSave() {
  const r = snapshot.calibration.run;
  if (!r?.saved || !r.result) return;
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
}
/** The saved points changed in Rust: the dashboard reloads them when `points_rev` moves. */
function setPoints(points: CalPoint[]) {
  CAL_CONFIG.calibration_points = points.sort((a, b) => a.speed - b.speed);
  snapshot.calibration.points_rev += 1;
}
function sweepCurrent() {
  const s = snapshot.calibration.sweep;
  if (!s) return null;
  const i = s.marks.indexOf("pending");
  s.current = i < 0 ? null : i;
  return s.current === null ? null : s.targets_kmh[s.current];
}
function runBelt(kmh: number) {
  Object.assign(snapshot.deck, { status: "Running", raw_speed_kmh: kmh, speed_kmh: kmh });
  taps = [];
  calRun(kmh);
}
/** ?calwizard: two saved points; the old app-wide belt length, 300 cm, prefills ?calwizard=belt. */
// ?calwizard=oldpoint adds a point stored at 0.15, below the limit.
// ?calwizard=measured starts with the 3.0 km/h run already averaged in (3 runs).
const CAL_CONFIG: { belt_length: number; calibration_points: CalPoint[] } = {
  belt_length: 300,
  calibration_points: [
    calFlag === "measured"
      ? { speed: 3, stride_length_cm: 68.4, distance_scale: (0.912 * 2 + 3 / 2.82) / 3, runs: 3 }
      : { speed: 3, stride_length_cm: 68.4, distance_scale: 0.912, runs: 2 },
    { speed: 4.5, stride_length_cm: 74.1, distance_scale: calFlag === "oldpoint" ? 0.15 : 0.887 },
  ],
};
/** The preview runs no settings sync: the store settings ?calwizard starts with. */
export function previewSettings() {
  return calFlag === null ? null : { beltLength: CAL_CONFIG.belt_length, calibrationPoints: CAL_CONFIG.calibration_points };
}
const CAL_COMMANDS: Partial<Record<Command["type"], (c: Command) => void>> = {
  calibration_set_wizard: (c) => {
    if (c.type !== "calibration_set_wizard") return;
    snapshot.calibration.wizard_enabled = c.on;
    if (!c.on) {
      snapshot.calibration.sweep = null;
      closeCal();
    }
  },
  calibration_run_start: (c) => {
    if (c.type !== "calibration_run_start") return;
    snapshot.calibration.sweep = null;
    runBelt(c.target_kmh);
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
    if (r.phase === "measured") {
      r.note = "The run is measured. TAP AGAIN starts it over.";
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
    if (calFlag === null) return;
    snapshot.calibration.wizard_enabled = calFlag !== "oldpoint";
    snapshot.calibration.belt_treadmill = true;
    snapshot.calibration.belt_cm = calFlag === "belt" ? null : BELT_CM;
    const live = ["tapping", "dropped", "measured"].includes(calFlag);
    if (live) Object.assign(snapshot.deck, { status: "Running", raw_speed_kmh: 3, speed_kmh: 3.3 });
    if (calFlag === "tapping") tappingRun(3, 5, 3);
    if (calFlag === "dropped") {
      tappingRun(3, 7, 3, { kept: 5, dropped: 1, note: "The speed changed (3.0 to 3.5 km/h), so tapping starts again." });
    }
    if (calFlag === "measured") {
      measured(3, 2.82, 3, 9, true);
      before = { speed: 3, stride_length_cm: 68.4, distance_scale: 0.912, runs: 2 };
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
