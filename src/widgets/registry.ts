import { formatClock, km, lifetimeKm } from "../format";
import type { AppSnapshot, HudWidget, PulseTarget, ZoneBand } from "../ipc";
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
  /** Themed long label shown on the dashboard card. */
  codename: string;
  accent: Accent;
  read: (s: AppSnapshot) => StatReading;
  /** A spark chart under the value, drawn from the snapshot; null shows none. */
  spark?: (s: AppSnapshot) => SparkData | null;
  /** A heart rate zone beside the value, with Rust's under/in/over; null shows none. */
  zone?: (s: AppSnapshot) => ZoneBand | null;
  /** A cool-down step's pulse target beside the value, with Rust's word for where the pulse is; null shows none. */
  target?: (s: AppSnapshot) => PulseTarget | null;
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
    codename: "PHYS_VELOCITY",
    accent: "cyan",
    read: (s) => ({ value: s.deck.speed_kmh.toFixed(1), unit: "KM/H" }),
  },
  {
    id: "INCLINE",
    label: "TILT",
    codename: "ALTITUDE_BIAS",
    accent: "magenta",
    read: (s) => ({ value: s.deck.incline_pct.toFixed(1), unit: "%" }),
  },
  {
    id: "TIMER",
    label: "TIME",
    codename: "DECK_CYCLES",
    accent: "cyan",
    read: (s) => ({ value: formatClock(s.session.duration_s) }),
  },
  {
    id: "DISTANCE",
    label: "DIST",
    codename: "DISPLACEMENT",
    accent: "cyan",
    read: (s) => ({ value: km(s.session.distance_m), unit: "KM" }),
  },
  {
    id: "STEPS",
    label: "STEPS",
    codename: "LOCOMOTION_INC",
    accent: "cyan",
    read: (s) => ({ value: s.session.steps.toString() }),
  },
  {
    id: "CALORIES",
    label: "BURN",
    codename: "METABOLIC_BURN",
    accent: "cyan",
    read: (s) => ({ value: Math.round(s.session.calories).toString(), unit: "KCAL" }),
  },
  {
    id: "DAILY",
    label: "TODAY",
    codename: "DAILY_TOTAL",
    accent: "green",
    read: (s) => ({ value: km(s.session.daily_distance_m), unit: "KM" }),
  },
  {
    id: "NEURAL",
    label: "FLOW",
    codename: "NEURAL_FLUIDITY",
    accent: "yellow",
    read: (s) => ({ value: Math.round(s.neural.factor * 100).toString(), unit: "%" }),
  },
  {
    id: "ODOMETER",
    label: "ODO",
    codename: "LIFETIME_ODOMETER",
    accent: "yellow",
    // Whole km at 1000 and up, so the HUD cell never outgrows its column.
    read: (s) => ({ value: lifetimeKm(s.odometer.distance_km), unit: "KM" }),
  },
  {
    id: "HEART_RATE",
    label: "HR",
    codename: "PULSE_RATE",
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
