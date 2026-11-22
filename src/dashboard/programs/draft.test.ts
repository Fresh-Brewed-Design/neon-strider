import { describe, expect, it } from "vitest";
import type { Program } from "../../ipc";
import { fromDraft, NEW_PROGRAM, pulseSummary, toDraft, withZoneUnit, zoneSummary } from "./draft";

const PROGRAM: Program = {
  id: 7,
  name: "Workday",
  steps: [
    { duration_minutes: 5, speed_kmh: 3, incline_pct: 2.5 },
    { duration_minutes: null, speed_kmh: 4.2, incline_pct: 0 },
  ],
  repeat_every_minutes: null,
};

describe("program drafts", () => {
  it("round-trips a saved program", () => {
    expect(fromDraft(toDraft(PROGRAM), 190)).toEqual(PROGRAM);
  });

  it("shows an open-ended step as open with a default length", () => {
    const d = toDraft(PROGRAM);
    expect(d.steps[1]).toMatchObject({ minutes: "10", open: true, speed: "4.2", incline: "0", zone: false });
  });

  it("trims the name and keeps the repeat interval", () => {
    const p = fromDraft({ ...NEW_PROGRAM, name: "  Post-meal ", repeat: true, repeatMinutes: "45" }, 190);
    expect(p).toMatchObject({ id: 0, name: "Post-meal", repeat_every_minutes: 45 });
  });

  it("names the step with an empty or non-numeric value", () => {
    const bad = { ...NEW_PROGRAM, steps: [...NEW_PROGRAM.steps, { ...NEW_PROGRAM.steps[0], minutes: "x" }] };
    expect(fromDraft(bad, 190)).toBe("Step 2 has an empty or non-numeric value.");
    expect(fromDraft({ ...NEW_PROGRAM, repeat: true, repeatMinutes: "" }, 190)).toBe("Enter how often the program repeats.");
  });

  // Regression: a save from the editor used to drop each step's zone.
  it("keeps a step's heart zone through the editor", () => {
    const zoned: Program = {
      ...PROGRAM,
      steps: [PROGRAM.steps[0], { ...PROGRAM.steps[1], zone: { low_pct: 85, high_pct: 95 } }],
    };
    expect(fromDraft(toDraft(zoned), 190)).toEqual(zoned);
    // A zone that trims incline keeps it through the editor, in either unit.
    const tilted: Program = { ...zoned, steps: [zoned.steps[0], { ...zoned.steps[1], zone: { low_pct: 85, high_pct: 95, trim: "incline" } }] };
    expect(toDraft(tilted).steps[1].zoneTrim).toBe("incline");
    expect(fromDraft(toDraft(tilted), 190)).toEqual(tilted);
    expect(fromDraft(withZoneUnit(toDraft(tilted), "bpm", 190), 190)).toEqual(tilted);
    // Switched off in the editor, the zone is gone; a plain step gains none.
    const d = toDraft(zoned);
    d.steps[1].zone = false;
    const saved = fromDraft(d, 190) as Program;
    expect(saved.steps[1].zone).toBeUndefined();
    expect("zone" in saved.steps[0]).toBe(false);
  });

  it("saves a zone typed in bpm as % of the max heart rate", () => {
    const d = { ...NEW_PROGRAM, zoneUnit: "bpm" as const, steps: [{ ...NEW_PROGRAM.steps[0], zone: true, zoneLow: "153", zoneHigh: "171" }] };
    expect((fromDraft(d, 180) as Program).steps[0].zone).toEqual({ low_pct: 85, high_pct: 95 });
  });

  it("converts the zones when the unit changes and back", () => {
    const d = { ...NEW_PROGRAM, steps: [{ ...NEW_PROGRAM.steps[0], zone: true, zoneLow: "85", zoneHigh: "95" }] };
    const bpm = withZoneUnit(d, "bpm", 190);
    expect(bpm.steps[0]).toMatchObject({ zoneLow: "162", zoneHigh: "181" });
    expect(withZoneUnit(bpm, "pct", 190).steps[0]).toMatchObject({ zoneLow: "85.3", zoneHigh: "95.3" });
    // A saved zone shown in bpm and left alone saves exactly as it was.
    const zoned: Program = { ...PROGRAM, steps: [{ ...PROGRAM.steps[0], zone: { low_pct: 85, high_pct: 95 } }] };
    const shown = withZoneUnit(toDraft(zoned), "bpm", 178);
    expect(shown.steps[0]).toMatchObject({ zoneLow: "151", zoneHigh: "169" });
    expect(fromDraft(shown, 178)).toEqual(zoned);
    expect(withZoneUnit(shown, "pct", 178).steps[0]).toMatchObject({ zoneLow: "85", zoneHigh: "95" });
    expect(zoneSummary(d.steps[0], "pct", 190)).toEqual({ pct: [85, 95], bpm: [162, 181] });
    expect(zoneSummary({ ...d.steps[0], zoneHigh: "" }, "pct", 190)).toBeNull();
  });

  it("names the step with a missing zone bound", () => {
    const d = { ...NEW_PROGRAM, steps: [{ ...NEW_PROGRAM.steps[0], zone: true, zoneLow: "" }] };
    expect(fromDraft(d, 190)).toBe("Step 1 needs both heart zone bounds.");
  });

  it("keeps a cool-down's pulse target through the editor, in either unit (#233)", () => {
    const cool: Program = {
      ...PROGRAM,
      steps: [PROGRAM.steps[0], { duration_minutes: 5, speed_kmh: 3, incline_pct: 0, kind: "cool_down", pulse_end: { below_pct: 60 } }],
    };
    expect(fromDraft(toDraft(cool), 190)).toEqual(cool);
    const bpm = withZoneUnit(toDraft(cool), "bpm", 178);
    expect(bpm.steps[1].pulseTarget).toBe("107");
    expect(fromDraft(bpm, 178)).toEqual(cool);
    expect(pulseSummary(toDraft(cool).steps[1], "pct", 190)).toEqual({ pct: 60, bpm: 114 });
    // Typed in bpm, saved as % of max.
    const typed = { ...bpm, steps: [bpm.steps[0], { ...bpm.steps[1], pulseTarget: "120" }] };
    expect((fromDraft(typed, 200) as Program).steps[1].pulse_end).toEqual({ below_pct: 60 });
    // A step switched to another kind, or switched off, drops the target.
    const walk = toDraft(cool);
    walk.steps[1].kind = "walk";
    expect((fromDraft(walk, 190) as Program).steps[1]).not.toHaveProperty("pulse_end");
    const off = toDraft(cool);
    off.steps[1].pulse = false;
    expect((fromDraft(off, 190) as Program).steps[1]).not.toHaveProperty("pulse_end");
    expect(fromDraft({ ...toDraft(cool), steps: [toDraft(cool).steps[0], { ...toDraft(cool).steps[1], pulseTarget: "" }] }, 190)).toBe(
      "Step 2 needs a pulse target.",
    );
  });
});
