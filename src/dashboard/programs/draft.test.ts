import { describe, expect, it } from "vitest";
import type { Program } from "../../ipc";
import { fromDraft, NEW_PROGRAM, toDraft } from "./draft";

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
    expect(fromDraft(toDraft(PROGRAM))).toEqual(PROGRAM);
  });

  it("shows an open-ended step as open with a default length", () => {
    const d = toDraft(PROGRAM);
    expect(d.steps[1]).toEqual({ minutes: "10", open: true, speed: "4.2", incline: "0" });
  });

  it("trims the name and keeps the repeat interval", () => {
    const p = fromDraft({ ...NEW_PROGRAM, name: "  Post-meal ", repeat: true, repeatMinutes: "45" });
    expect(p).toMatchObject({ id: 0, name: "Post-meal", repeat_every_minutes: 45 });
  });

  it("names the step with an empty or non-numeric value", () => {
    const bad = { ...NEW_PROGRAM, steps: [...NEW_PROGRAM.steps, { minutes: "x", open: false, speed: "3", incline: "0" }] };
    expect(fromDraft(bad)).toBe("Step 2 has an empty or non-numeric value.");
    expect(fromDraft({ ...NEW_PROGRAM, repeat: true, repeatMinutes: "" })).toBe("Enter how often the program repeats.");
  });
});
