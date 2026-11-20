// The program editor's form state. Numbers stay as typed text until saved.

import type { Program, ProgramStep } from "../../ipc";

/** How the editor shows heart rate zones: % of maximum heart rate (what Rust stores) or bpm. */
export type ZoneUnit = "pct" | "bpm";

export interface StepDraft {
  minutes: string;
  open: boolean;
  speed: string;
  incline: string;
  /** The step holds a heart rate zone. */
  zone: boolean;
  /** The zone's bounds as typed, in the draft's `zoneUnit`. */
  zoneLow: string;
  zoneHigh: string;
}

export interface Draft {
  id: number;
  name: string;
  steps: StepDraft[];
  repeat: boolean;
  repeatMinutes: string;
  zoneUnit: ZoneUnit;
}

/** A zone switched on for a step starts at 60 to 70 % of max, a steady aerobic walk. */
const ZONE_DEFAULT = { zoneLow: "60", zoneHigh: "70" };

export const NEW_PROGRAM: Draft = {
  id: 0,
  name: "",
  steps: [{ minutes: "10", open: false, speed: "3.0", incline: "0", zone: false, ...ZONE_DEFAULT }],
  repeat: false,
  repeatMinutes: "30",
  zoneUnit: "pct",
};

/** % of max to bpm, whole beats. */
const toBpm = (pct: number, maxBpm: number) => Math.round((pct / 100) * maxBpm);
/** bpm to % of max, to 0.1 %: fine enough that every whole bpm comes back as itself. */
const toPct = (bpm: number, maxBpm: number) => Math.round((bpm / maxBpm) * 1000) / 10;

export function toDraft(p: Program): Draft {
  return {
    id: p.id,
    name: p.name,
    steps: p.steps.map((s) => ({
      minutes: s.duration_minutes != null ? String(s.duration_minutes) : "10",
      open: s.duration_minutes == null,
      speed: s.speed_kmh.toFixed(1),
      incline: String(s.incline_pct),
      zone: s.zone != null,
      ...(s.zone ? { zoneLow: String(s.zone.low_pct), zoneHigh: String(s.zone.high_pct) } : ZONE_DEFAULT),
    })),
    repeat: p.repeat_every_minutes != null,
    repeatMinutes: p.repeat_every_minutes != null ? String(p.repeat_every_minutes) : "30",
    zoneUnit: "pct",
  };
}

/** Show the zones in `unit`, converting what was typed; text that isn't a number is left as typed. */
export function withZoneUnit(d: Draft, unit: ZoneUnit, maxBpm: number): Draft {
  if (unit === d.zoneUnit) return d;
  const convert = (text: string) => {
    const v = Number(text);
    if (text.trim() === "" || !Number.isFinite(v)) return text;
    return String(unit === "bpm" ? toBpm(v, maxBpm) : toPct(v, maxBpm));
  };
  return {
    ...d,
    zoneUnit: unit,
    steps: d.steps.map((s) => ({ ...s, zoneLow: convert(s.zoneLow), zoneHigh: convert(s.zoneHigh) })),
  };
}

/** A step's zone in both units for the editor's hint, or null while a bound isn't a number. */
export function zoneSummary(s: StepDraft, unit: ZoneUnit, maxBpm: number): { pct: [number, number]; bpm: [number, number] } | null {
  const low = Number(s.zoneLow);
  const high = Number(s.zoneHigh);
  if (s.zoneLow.trim() === "" || s.zoneHigh.trim() === "" || !Number.isFinite(low) || !Number.isFinite(high)) return null;
  if (unit === "bpm") return { bpm: [low, high], pct: [toPct(low, maxBpm), toPct(high, maxBpm)] };
  return { pct: [low, high], bpm: [toBpm(low, maxBpm), toBpm(high, maxBpm)] };
}

/**
 * Parse the form. Range rules are enforced by Rust on save; this only rejects non-numbers.
 * Zones typed in bpm are saved as % of `maxBpm`, the maximum heart rate Rust uses.
 */
export function fromDraft(d: Draft, maxBpm: number): Program | string {
  const n = (v: string) => (v.trim() === "" ? NaN : Number(v));
  const steps: ProgramStep[] = [];
  for (const [i, s] of d.steps.entries()) {
    const speed = n(s.speed);
    const incline = n(s.incline);
    const minutes = s.open ? null : n(s.minutes);
    if (Number.isNaN(speed) || Number.isNaN(incline) || (minutes !== null && Number.isNaN(minutes))) {
      return `Step ${i + 1} has an empty or non-numeric value.`;
    }
    const step: ProgramStep = { duration_minutes: minutes, speed_kmh: speed, incline_pct: incline };
    if (s.zone) {
      const low = n(s.zoneLow);
      const high = n(s.zoneHigh);
      if (Number.isNaN(low) || Number.isNaN(high)) return `Step ${i + 1} needs both heart zone bounds.`;
      step.zone =
        d.zoneUnit === "bpm"
          ? { low_pct: toPct(low, maxBpm), high_pct: toPct(high, maxBpm) }
          : { low_pct: low, high_pct: high };
    }
    steps.push(step);
  }
  const every = d.repeat ? n(d.repeatMinutes) : null;
  if (every !== null && Number.isNaN(every)) return "Enter how often the program repeats.";
  return { id: d.id, name: d.name.trim(), steps, repeat_every_minutes: every };
}
