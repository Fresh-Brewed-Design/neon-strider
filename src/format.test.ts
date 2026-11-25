import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { formatDayHeading, stasisClock, stasisLabel } from "./format";

// Clock changes in America/New_York: 2026-03-08 (23 h day) and 2026-11-01 (25 h day).
describe("formatDayHeading across a daylight-saving change", () => {
  const prevTz = process.env.TZ;
  beforeAll(() => {
    process.env.TZ = "America/New_York";
  });
  afterAll(() => {
    if (prevTz === undefined) delete process.env.TZ;
    else process.env.TZ = prevTz;
  });

  it("uses the New York zone", () => {
    expect(new Date(2026, 10, 1, 12).getTimezoneOffset()).toBe(300);
    expect(new Date(2026, 9, 31, 12).getTimezoneOffset()).toBe(240);
  });

  it("labels yesterday in the last hour of a 25 hour day (clocks go back)", () => {
    const now = new Date(2026, 10, 1, 23, 30).getTime();
    expect(formatDayHeading(new Date(2026, 9, 31, 12).getTime(), now)).toBe("Yesterday");
    expect(formatDayHeading(new Date(2026, 10, 1, 0, 10).getTime(), now)).toBe("Today");
  });

  it("labels yesterday in the first hour after a 23 hour day (clocks go forward)", () => {
    const now = new Date(2026, 2, 9, 0, 30).getTime();
    expect(formatDayHeading(new Date(2026, 2, 8, 12).getTime(), now)).toBe("Yesterday");
    expect(formatDayHeading(new Date(2026, 2, 7, 12).getTime(), now)).not.toBe("Yesterday");
  });

  it("labels yesterday just after the clocks go back", () => {
    const now = new Date(2026, 10, 2, 0, 30).getTime();
    expect(formatDayHeading(new Date(2026, 10, 1, 12).getTime(), now)).toBe("Yesterday");
  });
});

describe("the STASIS clock", () => {
  it("reads m:ss under an hour and h:mm:ss from an hour up", () => {
    expect(stasisClock(5)).toBe("0:05");
    expect(stasisClock(200.9)).toBe("3:20");
    expect(stasisClock(3600)).toBe("1:00:00");
    expect(stasisClock(3909)).toBe("1:05:09");
  });

  it("keeps the bare word until Rust has a clock", () => {
    expect(stasisLabel(null)).toBe("STASIS");
    expect(stasisLabel(65)).toBe("STASIS\u00a01:05");
  });
});
