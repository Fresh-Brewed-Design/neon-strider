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

/**
 * The patch for a step whose kind is set to `kind`. A step set to COOL_DOWN that can end on heart
 * rate (minutes, a walking speed, no zone: Rust's rules) gets the pulse target switched on at the
 * default, the one target every cool-down gets (docs/research.md); the user can untick it. Any other
 * change, and a step that was already COOL_DOWN, only sets the kind.
 */
export function kindPatch(s: StepDraft, kind: StepKind | null, unit: ZoneUnit, maxBpm: number): Partial<StepDraft> {
  if (kind !== "cool_down" || s.kind === "cool_down") return { kind };
  const speed = Number(s.speed);
  const qualifies = !s.open && !s.zone && s.speed.trim() !== "" && Number.isFinite(speed) && speed > 0;
  if (!qualifies) return { kind };
  const pct = SETTING_LIMITS.default_cool_down_pulse_pct;
  return { kind, pulse: true, pulseTarget: String(unit === "bpm" ? toBpm(pct, maxBpm) : pct), savedPulse: pct };
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

/**
 * STRIDE_FIT's flag on a step: the step's typed speed is above the fastest
 * speed the stride still fits the deck (`snapshot.deck.stride_fit_kmh`).
 * Compared in tenths, the step the deck runs and the editor shows, so a step
 * at the fit speed is not flagged. None while STRIDE_FIT is unset or the
 * speed isn't a number yet. Only a flag: nothing changes the step.
 */
export function strideFitFlag(speedText: string, fitKmh: number | null): string | null {
  if (fitKmh === null || speedText.trim() === "") return null;
  const v = Number(speedText);
  if (!Number.isFinite(v)) return null;
  const tenths = (x: number) => Math.round(x * 10);
  return tenths(v) > tenths(fitKmh) ? `Above STRIDE_FIT ${fitKmh.toFixed(1)} km/h` : null;
}

/**
 * The draft would save something other than what the editor opened with. Compared as the
 * programs they save, so a zone unit switch or "3" for "3.0" is no change; a draft that
 * doesn't parse yet is compared as typed.
 */
export function draftChanged(opened: Draft, d: Draft, maxBpm: number): boolean {
  const a = fromDraft(opened, maxBpm);
  const b = fromDraft(d, maxBpm);
  if (typeof a === "string" || typeof b === "string") return JSON.stringify(opened) !== JSON.stringify(d);
  return JSON.stringify(a) !== JSON.stringify(b);
}

/** A step's fields that can be wrong, each with the line shown under it. */
export interface StepErrors {
  minutes?: string;
  speed?: string;
  incline?: string;
  zone?: string;
  pulse?: string;
}

export interface DraftErrors {
  name?: string;
  steps: StepErrors[];
  repeat?: string;
}

/** A speed limit as text: to 0.1 km/h, or 0.01 when a calibrated limit needs it. */
const kmh = (v: number) => (Math.abs(v * 10 - Math.round(v * 10)) < 1e-6 ? v.toFixed(1) : v.toFixed(2));

/** The speeds a step may have besides 0, in calibrated km/h: Rust's floor and top. */
export interface SpeedRange {
  min: number;
  max: number;
}

/**
 * What the editor can tell is wrong before Rust sees the program, field by field. Rust still
 * checks everything on save. A field left empty only counts once `showEmpty` is set (after a
 * Save), so clearing a field to retype it shows nothing.
 */
export function draftErrors(d: Draft, range: SpeedRange | null, maxBpm: number, showEmpty: boolean): DraftErrors {
  const out: DraftErrors = { steps: [] };
  // A field's number, or why there isn't one: "empty" (only an error once shown) or "nan".
  const num = (text: string): number | "empty" | "nan" => {
    if (text.trim() === "") return "empty";
    const v = Number(text);
    return Number.isFinite(v) ? v : "nan";
  };
  const needs = (v: number | "empty" | "nan", text: string) => (v === "nan" || (v === "empty" && showEmpty) ? text : undefined);
  const pct = (v: number) => (d.zoneUnit === "bpm" ? (v / maxBpm) * 100 : v);

  if (showEmpty && d.name.trim() === "") out.name = "Enter a name.";

  for (const s of d.steps) {
    const e: StepErrors = {};
    if (!s.open) {
      const m = num(s.minutes);
      e.minutes = typeof m === "number" ? (m < 0 ? "Minutes can't be below 0." : undefined) : needs(m, "Minutes need a number.");
    }
    const v = num(s.speed);
    if (typeof v === "number") {
      // Rust's rule: 0, or from the floor (to the 0.01 km/h it is shown to) to the top.
      const outside = v !== 0 && (v < 0 || (range !== null && (v < range.min - 0.005 || v > range.max)));
      if (outside) e.speed = range ? `Speed must be 0 or ${kmh(range.min)} to ${kmh(range.max)} km/h.` : "Speed can't be below 0.";
    } else e.speed = needs(v, "Speed needs a number.");
    const inc = num(s.incline);
    e.incline = typeof inc === "number" ? (inc < 0 ? "Incline can't be below 0 %." : undefined) : needs(inc, "Incline needs a number.");
    if (s.zone) {
      const low = num(s.zoneLow);
      const high = num(s.zoneHigh);
      if (typeof low === "number" && typeof high === "number") {
        if (low >= high) e.zone = "Heart zone low must be less than high.";
      } else e.zone = needs(low === "nan" || high === "nan" ? "nan" : "empty", "Heart zone needs both bounds.");
    }
    if (s.pulse && s.kind === "cool_down") {
      const t = num(s.pulseTarget);
      const { min, max } = SETTING_LIMITS.cool_down_pulse_pct;
      if (typeof t === "number") {
        const p = pct(t);
        if (p < min - 0.05 || p > max + 0.05) {
          e.pulse =
            d.zoneUnit === "bpm"
              ? `Pulse target must be ${Math.ceil((min / 100) * maxBpm)} to ${Math.floor((max / 100) * maxBpm)} bpm.`
              : `Pulse target must be ${min} to ${max} % of max.`;
        }
      } else e.pulse = needs(t, "Pulse target needs a number.");
    }
    out.steps.push(Object.fromEntries(Object.entries(e).filter(([, text]) => text !== undefined)) as StepErrors);
  }

  if (d.repeat) {
    const r = num(d.repeatMinutes);
    out.repeat = typeof r === "number" ? (r < 0 ? "Repeat minutes can't be below 0." : undefined) : needs(r, "Repeat needs a number of minutes.");
    if (out.repeat === undefined) delete out.repeat;
  }
  return out;
}

/** Where the draft has errors, for Save's line ("step 2", "the name and steps 1 and 3"), or null. */
export function errorPlaces(e: DraftErrors): string | null {
  const steps = e.steps.flatMap((s, i) => (Object.keys(s).length > 0 ? [i + 1] : []));
  const list = (items: string[]) => (items.length < 2 ? items.join("") : `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`);
  const parts: string[] = [];
  if (e.name) parts.push("the name");
  if (steps.length > 0) parts.push(`${steps.length === 1 ? "step" : "steps"} ${list(steps.map(String))}`);
  if (e.repeat) parts.push("the repeat minutes");
  return parts.length > 0 ? list(parts) : null;
}

/** Rust's save error for one step ("Step 2: speed must be ..."), as its step and a sentence. */
export function rustStepError(message: string): { step: number; text: string } | null {
  const m = /^Step (\d+): (.+?)\.?$/.exec(message.trim());
  if (!m) return null;
  return { step: Number(m[1]), text: `${m[2].charAt(0).toUpperCase()}${m[2].slice(1)}.` };
}
