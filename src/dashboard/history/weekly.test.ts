import { describe, it, expect } from "vitest";
import { formatChange } from "./weekly";

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
