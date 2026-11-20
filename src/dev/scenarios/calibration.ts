// The guided calibration (Labs). ?calwizard: the switch is on, with two saved
// points. ?calwizard=counting: a count 23 s in (the clock ticks).
// ?calwizard=counted: a timed count waiting for the loops.
// ?calwizard=sweep: RUN ALL with two speeds done and 4.0 km/h up next;
// ?calwizard=sweeprun: the same sweep counting at 4.0. The math is a rough
// stand-in for Rust's (calibration.rs, unit-tested there).
import type { Command } from "../../ipc";
import { PREVIEW_CONFIG, params, publish, snapshot } from "../mockState";
import type { Scenario } from "./types";

const calFlag = params.get("calwizard");
let calTimer: ReturnType<typeof setInterval> | null = null;
function calRun(target_kmh: number, phase: "settling" | "counting" | "counted", elapsed_s = 0) {
  if (calTimer) clearInterval(calTimer);
  calTimer = null;
  snapshot.calibration.run = {
    target_kmh, phase, elapsed_s, reported_kmh: phase === "counted" ? target_kmh : null, result: null,
  };
  if (phase === "counting") {
    calTimer = setInterval(() => {
      const r = snapshot.calibration.run;
      if (r?.phase === "counting") r.elapsed_s += 0.25;
      publish();
    }, 250);
  }
}
function closeCal() {
  if (calTimer) clearInterval(calTimer);
  calTimer = null;
  snapshot.calibration.run = null;
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
  calRun(kmh, "settling");
}
/** ?calwizard: two saved points and a 3 m belt loop. */
// ?calwizard=oldpoint adds a point stored at 0.15, below the limit.
const CAL_CONFIG = {
  belt_length: 300,
  calibration_points: [
    { speed: 3, stride_length_cm: 68.4, distance_scale: 0.912, runs: 2 },
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
  // STOP: the belt stops; a run not yet timed closes.
  stop: () => {
    Object.assign(snapshot.deck, { status: "Stopped", raw_speed_kmh: 0, speed_kmh: 0 });
    if (snapshot.calibration.run?.phase !== "counted") closeCal();
  },
  calibration_count_start: () => {
    const r = snapshot.calibration.run;
    if (!r) throw "Press RUN on a speed first.";
    if (snapshot.deck.raw_speed_kmh < 0.05) throw "The belt is not moving. Press RUN, then COUNT once it is up to speed.";
    calRun(r.target_kmh, "counting");
  },
  calibration_count_stop: () => {
    const r = snapshot.calibration.run;
    if (!r || r.phase !== "counting") throw "Press COUNT first.";
    if (r.elapsed_s < 15) throw `Keep counting: at least 15 s, so far ${r.elapsed_s.toFixed(0)} s.`;
    if (snapshot.deck.raw_speed_kmh < 0.05) {
      calRun(r.target_kmh, "settling");
      throw "The belt stopped during the count. Run it again.";
    }
    calRun(r.target_kmh, "counted", r.elapsed_s);
  },
  calibration_run_count: (c) => {
    const r = snapshot.calibration.run;
    if (c.type !== "calibration_run_count" || !r?.reported_kmh) return;
    if (c.count.method === "steps_only") {
      // The factor stays the nearest saved point's; the stride is the distance it gives over the steps.
      const near = CAL_CONFIG.calibration_points.reduce((a, b) =>
        Math.abs(b.speed - r.reported_kmh!) < Math.abs(a.speed - r.reported_kmh!) ? b : a,
      );
      const reported_m = (r.reported_kmh / 3.6) * r.elapsed_s;
      const real_m = reported_m / near.distance_scale;
      const stride = (real_m / c.count.steps) * 100;
      if (c.count.steps < 10) throw "Count at least 10 steps.";
      if (stride < 20 || stride > 200) throw `The step count gives a ${stride.toFixed(0)} cm stride, outside 20 to 200 cm. Check the step count.`;
      // Rust averages the stride into the saved point at the target and keeps its factor.
      const saved = CAL_CONFIG.calibration_points.find((p) => Math.abs(p.speed - r.target_kmh) <= 0.05);
      const n = Math.max(1, saved?.runs ?? 1);
      r.result = {
        point: { speed: Math.round(r.reported_kmh * 10) / 10, stride_length_cm: stride, distance_scale: near.distance_scale, runs: 1 },
        real_kmh: (real_m / r.elapsed_s) * 3.6,
        real_m,
        reported_m,
        cadence_spm: (c.count.steps / r.elapsed_s) * 60,
        stride_measured: true,
        averaged: saved
          ? {
              speed: saved.speed,
              stride_length_cm: (saved.stride_length_cm * n + stride) / (n + 1),
              distance_scale: saved.distance_scale,
              runs: n + 1,
            }
          : null,
      };
      return;
    }
    if (c.count.method !== "loops") return;
    const { loops, belt_length_cm, steps } = c.count;
    if (!(loops > 0)) throw "Enter how many loops you counted.";
    const real_m = (loops * belt_length_cm) / 100;
    const reported_m = (r.reported_kmh / 3.6) * r.elapsed_s;
    const scale = reported_m / real_m;
    if (scale < 0.2 || scale > 3) throw `That gives a factor of ${scale.toFixed(2)}, outside 0.2 to 3. Check the count and run it again.`;
    const stride = steps ? (real_m / steps) * 100 : 74;
    // Rust averages the run into the saved point at its target (calibration.rs averaged_point).
    const saved = CAL_CONFIG.calibration_points.find((p) => Math.abs(p.speed - r.target_kmh) <= 0.05);
    const n = Math.max(1, saved?.runs ?? 1);
    const mean = (old: number, next: number) => (old * n + next) / (n + 1);
    r.result = {
      point: { speed: Math.round(r.reported_kmh * 10) / 10, stride_length_cm: stride, distance_scale: scale, runs: 1 },
      real_kmh: (real_m / r.elapsed_s) * 3.6,
      real_m,
      reported_m,
      cadence_spm: steps ? (steps / r.elapsed_s) * 60 : null,
      stride_measured: steps !== null,
      averaged: saved
        ? {
            speed: saved.speed,
            stride_length_cm: steps ? mean(saved.stride_length_cm, stride) : saved.stride_length_cm,
            distance_scale: mean(saved.distance_scale, scale),
            runs: n + 1,
          }
        : null,
    };
  },
  calibration_run_close: () => closeCal(),
};

export const calibration: Scenario = {
  apply() {
    if (calFlag === null) return;
    snapshot.calibration.wizard_enabled = calFlag !== "oldpoint";
    if (calFlag === "counting" || calFlag === "counted") Object.assign(snapshot.deck, { raw_speed_kmh: 3, speed_kmh: 3.3 });
    if (calFlag === "counting") calRun(3, "counting", 23);
    if (calFlag === "counted") calRun(3, "counted", 62.4);
    // A stored 14.0 km/h the treadmill (12.0 max) can't run any more, and a 3.5 he added.
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
        calRun(4, "counting", 31);
      }
    }
  },
  commands: CAL_COMMANDS,
  queries: calFlag !== null ? { get_rust_config: () => ({ ...PREVIEW_CONFIG, ...CAL_CONFIG }) } : {},
};
