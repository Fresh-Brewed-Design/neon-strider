// The program editor's form state. Numbers stay as typed text until saved.

import { SETTING_LIMITS, type Program, type ProgramStep, type StepKind, type ZoneTrim } from "../../ipc";

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
  /** The zone as saved (% of max), so a bpm round trip that changes nothing saves it unchanged. */
  savedZone?: { low_pct: number; high_pct: number };
  /** What the zone trims with System -> Labs ZONE_TILT on; speed is the default Rust stores as none. */
  zoneTrim: ZoneTrim;
  /** What the step is for (WARM_UP, INTERVAL, ...); null for a step with no kind. */
  kind: StepKind | null;
  /** A COOL_DOWN step walks on after its minutes until the pulse is down (#233). */
  pulse: boolean;
  /** The pulse target as typed, in the draft's `zoneUnit`. */
  pulseTarget: string;
  /** The target as saved (% of max), so a bpm round trip that changes nothing saves it unchanged. */
  savedPulse?: number;
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
const ZONE_DEFAULT: Pick<StepDraft, "zoneLow" | "zoneHigh" | "zoneTrim"> = { zoneLow: "60", zoneHigh: "70", zoneTrim: "speed" };

/** A cool-down's pulse target starts at Rust's default share of max (60 %, inside the light band). */
const PULSE_DEFAULT: Pick<StepDraft, "pulse" | "pulseTarget"> = {
  pulse: false,
  pulseTarget: String(SETTING_LIMITS.default_cool_down_pulse_pct),
};

export const NEW_PROGRAM: Draft = {
  id: 0,
  name: "",
  steps: [{ minutes: "10", open: false, speed: "3.0", incline: "0", zone: false, ...ZONE_DEFAULT, kind: null, ...PULSE_DEFAULT }],
  repeat: false,
  repeatMinutes: "30",
  zoneUnit: "pct",
};

/** bpm back to % of max: the saved % when the bpm is still what it showed as, so nothing drifts. */
function pctFromBpm(bpm: number, saved: number | undefined, maxBpm: number): number {
  return saved !== undefined && toBpm(saved, maxBpm) === bpm ? saved : toPct(bpm, maxBpm);
}

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
      ...(s.zone
        ? {
            zoneLow: String(s.zone.low_pct),
            zoneHigh: String(s.zone.high_pct),
            savedZone: { low_pct: s.zone.low_pct, high_pct: s.zone.high_pct },
            zoneTrim: s.zone.trim ?? "speed",
          }
        : ZONE_DEFAULT),
      kind: s.kind ?? null,
      ...(s.pulse_end
        ? { pulse: true, pulseTarget: String(s.pulse_end.below_pct), savedPulse: s.pulse_end.below_pct }
        : PULSE_DEFAULT),
    })),
    repeat: p.repeat_every_minutes != null,
    repeatMinutes: p.repeat_every_minutes != null ? String(p.repeat_every_minutes) : "30",
    zoneUnit: "pct",
  };
}

/** Show the zones in `unit`, converting what was typed; text that isn't a number is left as typed. */
export function withZoneUnit(d: Draft, unit: ZoneUnit, maxBpm: number): Draft {
  if (unit === d.zoneUnit) return d;
  const convert = (text: string, saved: number | undefined) => {
    const v = Number(text);
    if (text.trim() === "" || !Number.isFinite(v)) return text;
    if (unit === "bpm") return String(toBpm(v, maxBpm));
    return String(pctFromBpm(v, saved, maxBpm));
  };
  return {
    ...d,
    zoneUnit: unit,
    steps: d.steps.map((s) => ({
      ...s,
      zoneLow: convert(s.zoneLow, s.savedZone?.low_pct),
      zoneHigh: convert(s.zoneHigh, s.savedZone?.high_pct),
      pulseTarget: convert(s.pulseTarget, s.savedPulse),
    })),
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

/** A cool-down's pulse target in both units for the editor's hint, or null while it isn't a number. */
export function pulseSummary(s: StepDraft, unit: ZoneUnit, maxBpm: number): { pct: number; bpm: number } | null {
  const v = Number(s.pulseTarget);
  if (s.pulseTarget.trim() === "" || !Number.isFinite(v)) return null;
  return unit === "bpm" ? { bpm: v, pct: toPct(v, maxBpm) } : { pct: v, bpm: toBpm(v, maxBpm) };
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
    // No kind is left out, as Rust stores it.
    if (s.kind) step.kind = s.kind;
    if (s.zone) {
      const low = n(s.zoneLow);
      const high = n(s.zoneHigh);
      if (Number.isNaN(low) || Number.isNaN(high)) return `Step ${i + 1} needs both heart zone bounds.`;
      step.zone =
        d.zoneUnit === "bpm"
          ? { low_pct: pctFromBpm(low, s.savedZone?.low_pct, maxBpm), high_pct: pctFromBpm(high, s.savedZone?.high_pct, maxBpm) }
          : { low_pct: low, high_pct: high };
      // Speed is the default: left out, as Rust stores it.
      if (s.zoneTrim !== "speed") step.zone.trim = s.zoneTrim;
    }
    // Only a COOL_DOWN step keeps a pulse end: a step switched to another kind drops it.
    if (s.pulse && s.kind === "cool_down") {
      const target = n(s.pulseTarget);
      if (Number.isNaN(target)) return `Step ${i + 1} needs a pulse target.`;
      step.pulse_end = { below_pct: d.zoneUnit === "bpm" ? pctFromBpm(target, s.savedPulse, maxBpm) : target };
    }
    steps.push(step);
  }
  const every = d.repeat ? n(d.repeatMinutes) : null;
  if (every !== null && Number.isNaN(every)) return "Enter how often the program repeats.";
  return { id: d.id, name: d.name.trim(), steps, repeat_every_minutes: every };
}
