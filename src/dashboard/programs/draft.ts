// The program editor's form state. Numbers stay as typed text until saved.

import type { Program, ProgramStep } from "../../ipc";

export interface StepDraft {
  minutes: string;
  open: boolean;
  speed: string;
  incline: string;
}

export interface Draft {
  id: number;
  name: string;
  steps: StepDraft[];
  repeat: boolean;
  repeatMinutes: string;
}

export const NEW_PROGRAM: Draft = {
  id: 0,
  name: "",
  steps: [{ minutes: "10", open: false, speed: "3.0", incline: "0" }],
  repeat: false,
  repeatMinutes: "30",
};

export function toDraft(p: Program): Draft {
  return {
    id: p.id,
    name: p.name,
    steps: p.steps.map((s) => ({
      minutes: s.duration_minutes != null ? String(s.duration_minutes) : "10",
      open: s.duration_minutes == null,
      speed: s.speed_kmh.toFixed(1),
      incline: String(s.incline_pct),
    })),
    repeat: p.repeat_every_minutes != null,
    repeatMinutes: p.repeat_every_minutes != null ? String(p.repeat_every_minutes) : "30",
  };
}

/** Parse the form. Range rules are enforced by Rust on save; this only rejects non-numbers. */
export function fromDraft(d: Draft): Program | string {
  const n = (v: string) => (v.trim() === "" ? NaN : Number(v));
  const steps: ProgramStep[] = [];
  for (const [i, s] of d.steps.entries()) {
    const speed = n(s.speed);
    const incline = n(s.incline);
    const minutes = s.open ? null : n(s.minutes);
    if (Number.isNaN(speed) || Number.isNaN(incline) || (minutes !== null && Number.isNaN(minutes))) {
      return `Step ${i + 1} has an empty or non-numeric value.`;
    }
    steps.push({ duration_minutes: minutes, speed_kmh: speed, incline_pct: incline });
  }
  const every = d.repeat ? n(d.repeatMinutes) : null;
  if (every !== null && Number.isNaN(every)) return "Enter how often the program repeats.";
  return { id: d.id, name: d.name.trim(), steps, repeat_every_minutes: every };
}
