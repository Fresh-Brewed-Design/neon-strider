import { describe, expect, it } from "vitest";
import { fromDateInput, toDateInput } from "./maintenance";

describe("date input helpers", () => {
  it("round-trips a local date", () => {
    const ms = new Date(2026, 2, 9).getTime();
    expect(toDateInput(ms)).toBe("2026-03-09");
    expect(fromDateInput("2026-03-09")).toBe(ms);
  });

  it("rejects anything that is not YYYY-MM-DD", () => {
    expect(fromDateInput("")).toBeNull();
    expect(fromDateInput("9/3/2026")).toBeNull();
  });
});
