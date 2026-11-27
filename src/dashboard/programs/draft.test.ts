import { describe, expect, it } from "vitest";
import type { Program } from "../../ipc";
import {
  draftChanged,
  draftErrors,
  errorPlaces,
  fromDraft,
  NEW_PROGRAM,
  pulseSummary,
  rustStepError,
  strideFitFlag,
  toClone,
  toDraft,
  withZoneUnit,
  zoneSummary,
  type StepDraft,
} from "./draft";

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

describe("strideFitFlag", () => {
  it("flags only a speed above STRIDE_FIT, in tenths", () => {
    expect(strideFitFlag("5.5", 5.4)).toBe("Above STRIDE_FIT 5.4 km/h");
    expect(strideFitFlag("5.4", 5.4)).toBeNull();
    expect(strideFitFlag("5.44", 5.4)).toBeNull();
    expect(strideFitFlag("3", 5.4)).toBeNull();
    expect(strideFitFlag("6", null)).toBeNull();
    expect(strideFitFlag("", 5.4)).toBeNull();
    expect(strideFitFlag("fast", 5.4)).toBeNull();
  });
});

describe("draftChanged", () => {
  it("sees no change in a draft saved as it opened", () => {
    const d = toDraft(PROGRAM);
    expect(draftChanged(d, structuredClone(d), 190)).toBe(false);
    // "3" for "3.0" and a zone unit switch save the same program.
    const same = structuredClone(d);
    same.steps[0].speed = "3";
    expect(draftChanged(d, withZoneUnit(same, "bpm", 190), 190)).toBe(false);
  });

  it("sees an edit, and an edit typed back as no change", () => {
    const d = toDraft(PROGRAM);
    const edited = structuredClone(d);
    edited.steps[0].speed = "3.5";
    expect(draftChanged(d, edited, 190)).toBe(true);
    edited.steps[0].speed = "3.0";
    expect(draftChanged(d, edited, 190)).toBe(false);
  });

  it("counts text that doesn't parse yet as a change", () => {
    const d = toDraft(PROGRAM);
    expect(draftChanged(d, { ...d, steps: [{ ...d.steps[0], speed: "abc" }, d.steps[1]] }, 190)).toBe(true);
  });
});

describe("draftErrors", () => {
  const range = { min: 0.6, max: 10.5 };
  const step = (patch: Partial<StepDraft>) => ({ ...NEW_PROGRAM, name: "Walk", steps: [{ ...NEW_PROGRAM.steps[0], ...patch }] });

  it("finds nothing wrong with a valid draft", () => {
    const e = draftErrors(toDraft(PROGRAM), range, 190, true);
    expect(e).toEqual({ steps: [{}, {}] });
    expect(errorPlaces(e)).toBeNull();
  });

  it("flags text that isn't a number at its field", () => {
    expect(draftErrors(step({ speed: "abc", minutes: "x", incline: "?" }), range, 190, false).steps[0]).toEqual({
      minutes: "Minutes need a number.",
      speed: "Speed needs a number.",
      incline: "Incline needs a number.",
    });
  });

  it("flags a speed outside the deck's range, and lets 0 through", () => {
    expect(draftErrors(step({ speed: "99" }), range, 190, false).steps[0].speed).toBe("Speed must be 0 or 0.6 to 10.5 km/h.");
    expect(draftErrors(step({ speed: "0.3" }), range, 190, false).steps[0].speed).toBe("Speed must be 0 or 0.6 to 10.5 km/h.");
    expect(draftErrors(step({ speed: "0" }), range, 190, false).steps[0]).toEqual({});
    // A calibrated floor shows to 0.01 km/h; the floor as shown is in range.
    expect(draftErrors(step({ speed: "0.63" }), { min: 0.634, max: 6 }, 190, false).steps[0]).toEqual({});
    expect(draftErrors(step({ speed: "0.5" }), { min: 0.634, max: 6 }, 190, false).steps[0].speed).toBe(
      "Speed must be 0 or 0.63 to 6.0 km/h.",
    );
  });

  it("only counts an empty field once Save was pressed", () => {
    const d = { ...step({ speed: "" }), name: "" };
    expect(draftErrors(d, range, 190, false)).toEqual({ steps: [{}] });
    const e = draftErrors(d, range, 190, true);
    expect(e).toEqual({ name: "Enter a name.", steps: [{ speed: "Speed needs a number." }] });
    expect(errorPlaces(e)).toBe("the name and step 1");
  });

  it("checks a zone's bounds and a pulse target in the unit shown", () => {
    expect(draftErrors(step({ zone: true, zoneLow: "70", zoneHigh: "60" }), range, 190, false).steps[0].zone).toBe(
      "Heart zone low must be less than high.",
    );
    expect(draftErrors(step({ zone: true, zoneLow: "70", zoneHigh: "" }), range, 190, true).steps[0].zone).toBe(
      "Heart zone needs both bounds.",
    );
    const cool = { kind: "cool_down" as const, pulse: true };
    expect(draftErrors(step({ ...cool, pulseTarget: "90" }), range, 190, false).steps[0].pulse).toBe("Pulse target must be 50 to 76 % of max.");
    const bpm = { ...step({ ...cool, pulseTarget: "200" }), zoneUnit: "bpm" as const };
    expect(draftErrors(bpm, range, 190, false).steps[0].pulse).toBe("Pulse target must be 95 to 144 bpm.");
    // A pulse target on a step that is no longer COOL_DOWN isn't saved, so it isn't checked.
    expect(draftErrors(step({ pulse: true, pulseTarget: "abc" }), range, 190, false).steps[0]).toEqual({});
  });

  it("checks the repeat minutes only while repeat is on", () => {
    expect(draftErrors({ ...step({}), repeatMinutes: "abc" }, range, 190, false).repeat).toBeUndefined();
    expect(draftErrors({ ...step({}), repeat: true, repeatMinutes: "abc" }, range, 190, false).repeat).toBe(
      "Repeat needs a number of minutes.",
    );
  });

  it("lists the steps with errors for Save's line", () => {
    expect(errorPlaces({ steps: [{ speed: "x" }, {}, { minutes: "y" }] })).toBe("steps 1 and 3");
    expect(errorPlaces({ steps: [{}], repeat: "x" })).toBe("the repeat minutes");
  });
});

describe("rustStepError", () => {
  it("splits Rust's step error into its step and a sentence", () => {
    expect(rustStepError("Step 2: a zone needs a walking speed")).toEqual({ step: 2, text: "A zone needs a walking speed." });
    expect(rustStepError("Name is required")).toBeNull();
  });
});

describe("toClone", () => {
  const ZONED: Program = {
    id: 9,
    name: "Climb",
    steps: [
      { duration_minutes: 5, speed_kmh: 4.5, incline_pct: 0, kind: "warm_up" },
      { duration_minutes: 4, speed_kmh: 6, incline_pct: 10, zone: { low_pct: 85, high_pct: 95 }, kind: "interval" },
    ],
    repeat_every_minutes: 30,
    note: "Kept by Rust, not by a clone.",
  };

  it("is a new program with the same steps, zones and repeat", () => {
    const d = toClone(ZONED);
    expect(d.id).toBe(0);
    const saved = fromDraft(d, 190);
    expect(saved).toEqual({ ...ZONED, id: 0, name: "Climb // CLONE", note: undefined });
    expect(typeof saved !== "string" && saved.steps).toEqual(ZONED.steps);
    expect(d.repeat).toBe(true);
    expect(d.repeatMinutes).toBe("30");
  });

  it("leaves the original alone", () => {
    const before = structuredClone(ZONED);
    toClone(ZONED);
    expect(ZONED).toEqual(before);
  });

  it("keeps its editor apart from a New draft's", () => {
    expect(toClone(ZONED).from).toBe(9);
    expect(NEW_PROGRAM.from).toBeUndefined();
  });

  it("cuts a long name so the whole name fits Rust's limit", () => {
    const d = toClone({ ...ZONED, name: "A".repeat(40) });
    expect(d.name).toBe(`${"A".repeat(31)} // CLONE`);
    expect(d.name).toHaveLength(40);
    expect(toClone({ ...ZONED, name: `${"A".repeat(30)} ${"B".repeat(9)}` }).name).toBe(`${"A".repeat(30)} // CLONE`);
  });
});
