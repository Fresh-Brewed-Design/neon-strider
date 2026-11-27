import { describe, it, expect } from "vitest";
import { formatChange, formatDeskDays } from "./weekly";

describe("formatChange", () => {
  it("shows direction and whole percent", () => {
    expect(formatChange({ pct: 12, is_new: false })).toEqual({ text: "▲ 12%", tone: "up" });
    expect(formatChange({ pct: -8, is_new: false })).toEqual({ text: "▼ 8%", tone: "down" });
    expect(formatChange({ pct: 0, is_new: false })).toEqual({ text: "±0%", tone: "flat" });
  });

  it("says NEW from zero, and shows nothing when there is nothing to compare", () => {
    expect(formatChange({ pct: null, is_new: true })).toEqual({ text: "NEW", tone: "new" });
    expect(formatChange({ pct: null, is_new: false })).toBeNull();
  });

  it("caps huge percentages so they still fit", () => {
    expect(formatChange({ pct: 12000, is_new: false })?.text).toBe("▲ 999%+");
    expect(formatChange({ pct: -100, is_new: false })?.text).toBe("▼ 100%");
  });
});

describe("formatDeskDays", () => {
  const week = { distance_km: 3, duration_s: 1800, active_days: 4, desk_days: 5, sessions: 4, calories: 200, longest_day_km: 1 };

  it("counts walked days out of desk days, never calendar days", () => {
    expect(formatDeskDays(week)).toBe("4 / 5 desk days");
    expect(formatDeskDays({ ...week, active_days: 1, desk_days: 1 })).toBe("1 / 1 desk day");
  });

  it("says so when the week has no desk days yet", () => {
    expect(formatDeskDays({ ...week, active_days: 0, desk_days: 0 })).toBe("No desk days yet");
  });
});
