import { formatClock, formatDuration, km, lifetimeKm } from "../format";
import type { AppSnapshot, HudWidget, PulseTarget, ZoneBand } from "../ipc";
import { PLAIN } from "../names";
import type { SparkData } from "./spark";

export type Accent = "cyan" | "magenta" | "yellow" | "green";

export interface StatReading {
  value: string;
  unit?: string;
}

export interface StatWidget {
  id: HudWidget;
  /** Short label shown on the HUD. */
  label: string;
  /** The stat's one name: the Live tile's title and the HUD page's row. */
  codename: string;
  /** The plain meaning, the small grey line under the codename; none where the codename is already plain. */
  plain?: string;
  accent: Accent;
  read: (s: AppSnapshot) => StatReading;
  /** A spark chart under the value, drawn from the snapshot; null shows none. */
  spark?: (s: AppSnapshot) => SparkData | null;
  /** A heart rate zone beside the value, with Rust's under/in/over; null shows none. */
  zone?: (s: AppSnapshot) => ZoneBand | null;
  /** A cool-down step's pulse target beside the value, with Rust's word for where the pulse is; null shows none. */
  target?: (s: AppSnapshot) => PulseTarget | null;
  /** A short line under the value; null shows none. */
  detail?: (s: AppSnapshot) => string | null;
}

/**
 * Every stat the app can show, defined once. The dashboard cards and the HUD
 * both render from this list, so a stat can't drift between the two.
 * CONTROLS and PROGRAMS are interactive modules, not stats, and are rendered
 * by their own components.
 */
export const STAT_WIDGETS: StatWidget[] = [
  {
    id: "VELOCITY",
    label: "VEL",
    codename: "VELOCITY",
    plain: PLAIN.VELOCITY,
    accent: "cyan",
    read: (s) => ({ value: s.deck.speed_kmh.toFixed(1), unit: "KM/H" }),
  },
  {
    id: "INCLINE",
    label: "TILT",
    codename: "TILT",
    plain: PLAIN.TILT,
    accent: "magenta",
    read: (s) => ({ value: s.deck.incline_pct.toFixed(1), unit: "%" }),
  },
  {
    id: "TIMER",
    label: "TIME",
    codename: "DECK_CYCLES",
    plain: PLAIN.DECK_CYCLES,
    accent: "cyan",
    read: (s) => ({ value: formatClock(s.session.duration_s) }),
  },
  {
    id: "DISTANCE",
    label: "DIST",
    codename: "DISPLACEMENT",
    plain: PLAIN.DISPLACEMENT,
    accent: "cyan",
    read: (s) => ({ value: km(s.session.distance_m), unit: "KM" }),
  },
  {
    id: "STEPS",
    label: "STEPS",
    codename: "LOCOMOTION_INC",
    plain: PLAIN.LOCOMOTION_INC,
    accent: "cyan",
    read: (s) => ({ value: s.session.steps.toString() }),
  },
  {
    id: "CALORIES",
    label: "BURN",
    codename: "METABOLIC_BURN",
    plain: PLAIN.METABOLIC_BURN,
    accent: "cyan",
    read: (s) => ({ value: Math.round(s.session.calories).toString(), unit: "KCAL" }),
  },
  {
    id: "DAILY",
    label: "TODAY",
    codename: "DAILY_TOTAL",
    plain: PLAIN.DAILY_TOTAL,
    accent: "green",
    read: (s) => ({ value: km(s.session.daily_distance_m), unit: "KM" }),
  },
  {
    id: "NEURAL",
    label: "FLOW",
    codename: "NEURAL_FLUIDITY",
    plain: PLAIN.NEURAL_FLUIDITY,
    accent: "yellow",
    read: (s) => ({ value: Math.round(s.neural.factor * 100).toString(), unit: "%" }),
  },
  {
    id: "ODOMETER",
    label: "ODO",
    codename: "LIFETIME_ODOMETER",
    plain: PLAIN.LIFETIME_ODOMETER,
    accent: "yellow",
    // Whole km at 1000 and up, so the HUD cell never outgrows its column.
    read: (s) => ({ value: lifetimeKm(s.odometer.distance_km), unit: "KM" }),
  },
  {
    id: "PC_WALK_SHARE",
    label: "WALKED",
    // Desk time, like DESK_SYNC's desk days: the computer in use, on any OS.
    codename: "DESK TIME WALKED",
    accent: "green",
    read: (s) => pcWalkShareReading(s),
    detail: (s) => pcWalkShareDetail(s),
  },
  {
    id: "HEART_RATE",
    label: "HR",
    codename: "PULSE_RATE",
    plain: PLAIN.PULSE_RATE,
    accent: "magenta",
    // "--" until a sensor is picked and sending.
    read: (s) => ({ value: s.heart_rate.bpm === null ? "--" : s.heart_rate.bpm.toString(), unit: "BPM" }),
    // Rust sends no history while no sensor is picked: no chart.
    spark: (s) =>
      s.heart_rate.history.length > 0
        ? { points: s.heart_rate.history, zone: s.heart_rate.zone, target: s.heart_rate.cool_down?.target_bpm ?? null }
        : null,
    // Rust sends a zone only while a zone step runs with a sensor picked.
    zone: (s) => s.heart_rate.zone,
    // And a pulse target only while a cool-down step that ends on heart rate runs.
    target: (s) => s.heart_rate.cool_down,
  },
];

/** Today's walked share of active PC time; "--" until Rust has enough PC time to give one. */
export function pcWalkShareReading(s: AppSnapshot): StatReading {
  const pct = s.pc_walk_share.share_pct;
  return { value: pct === null ? "--" : pct.toString(), unit: "%" };
}

/** Walked time of today's PC time, e.g. "2h 10m of 5h 40m"; none before the PC is used today. */
export function pcWalkShareDetail(s: AppSnapshot): string | null {
  const { active_s, walked_s } = s.pc_walk_share;
  if (active_s <= 0) return null;
  // Each duration stays on one line: a non-breaking space inside it, a plain one around "of".
  const whole = (seconds: number) => formatDuration(seconds).replace(" ", "\u00a0");
  return `${whole(walked_s)} of ${whole(active_s)}`;
}

/** A stat's tooltip where only its short label shows: "DECK_CYCLES: walk time". */
export function statTitle(w: StatWidget): string {
  return w.plain ? `${w.codename}: ${w.plain}` : w.codename;
}

const BY_ID = new Map(STAT_WIDGETS.map((w) => [w.id, w]));

export function statWidget(id: HudWidget): StatWidget | undefined {
  return BY_ID.get(id);
}

export interface ModuleWidget {
  id: HudWidget;
  label: string;
  description: string;
}

/** Interactive HUD modules (not single values). */
export const MODULE_WIDGETS: ModuleWidget[] = [
  { id: "CONTROLS", label: "Controls", description: "Start, pause, stop and speed/tilt steppers" },
  { id: "PROGRAMS", label: "Programs", description: "One-click program buttons and the running program" },
];

export function widgetName(id: HudWidget): string {
  return statWidget(id)?.codename ?? MODULE_WIDGETS.find((m) => m.id === id)?.label ?? id;
}
