import { describe, expect, it } from "vitest";
import { snapshot } from "../dev/mockBackend";
import { lifetimeKm } from "../format";
import { statWidget, widgetName } from "./registry";

describe("ODOMETER widget", () => {
  it("is a stat widget with the ODO label and LIFETIME_ODOMETER codename", () => {
    const w = statWidget("ODOMETER");
    expect(w?.label).toBe("ODO");
    expect(w?.codename).toBe("LIFETIME_ODOMETER");
    expect(widgetName("ODOMETER")).toBe("LIFETIME_ODOMETER");
  });

  it("reads the lifetime distance in km from the snapshot", () => {
    const reading = statWidget("ODOMETER")?.read({
      ...snapshot,
      odometer: { distance_km: 412.64, belt_hours: 138.2, since_ms: null },
    });
    expect(reading).toEqual({ value: "412.6", unit: "KM" });
  });

  it("drops the decimal from 1000 km so the HUD cell stays narrow", () => {
    expect(lifetimeKm(0)).toBe("0.0");
    expect(lifetimeKm(999.94)).toBe("999.9");
    expect(lifetimeKm(1000)).toBe("1000");
    expect(lifetimeKm(12345.6)).toBe("12346");
  });
});

describe("HEART_RATE widget", () => {
  it("is a stat widget called PULSE_RATE", () => {
    expect(statWidget("HEART_RATE")?.label).toBe("HR");
    expect(widgetName("HEART_RATE")).toBe("PULSE_RATE");
  });

  it("shows dashes until the sensor sends, then the bpm", () => {
    const w = statWidget("HEART_RATE");
    expect(w?.read(snapshot)).toEqual({ value: "--", unit: "BPM" });
    const live = { ...snapshot, heart_rate: { ...snapshot.heart_rate, bpm: 112 } };
    expect(w?.read(live)).toEqual({ value: "112", unit: "BPM" });
  });
});

describe("PC_WALK_SHARE widget", () => {
  const withShare = (active_s: number, walked_s: number, share_pct: number | null) => ({
    ...snapshot,
    pc_walk_share: { active_s, walked_s, share_pct },
  });

  it("is a stat widget called DESK TIME WALKED, shown as WALKED", () => {
    expect(statWidget("PC_WALK_SHARE")?.label).toBe("WALKED");
    expect(widgetName("PC_WALK_SHARE")).toBe("DESK TIME WALKED");
  });

  it("shows Rust's whole percent, or dashes while there is too little PC time", () => {
    const w = statWidget("PC_WALK_SHARE");
    expect(w?.read(withShare(20_400, 7_800, 38))).toEqual({ value: "38", unit: "%" });
    expect(w?.read(withShare(360, 240, null))).toEqual({ value: "--", unit: "%" });
    expect(w?.read(withShare(3_600, 0, 0))).toEqual({ value: "0", unit: "%" });
  });

  it("says how much of the PC time was walked, and nothing before the PC is used", () => {
    const w = statWidget("PC_WALK_SHARE");
    expect(w?.detail?.(withShare(20_400, 7_800, 38))).toBe("2h 10m of 5h 40m");
    expect(w?.detail?.(withShare(360, 0, null))).toBe("0s of 6m");
    expect(w?.detail?.(withShare(0, 0, null))).toBeNull();
  });

  it("is not on the HUD until picked", () => {
    expect(snapshot.hud.widgets).not.toContain("PC_WALK_SHARE");
  });
});
